//! Tauri adapter 组装根（概要 §6，开发计划 :77）。
//!
//! ## 这个模块负责什么
//!
//! 桌面形态下 **只有 adapter 能构造身份**（概要 §6.1 / CM §3 INV-01）。本模块把
//! 「当前桌面进程 + 当前登录用户 + 当前前端实例」组装成应用层认得的
//! [`RequestContext`]（[`identity`]），把归属意图组装成 [`OwnerRef`] 并在 adapter 层
//! 做归属校验（概要 §5.1(3)），再把应用层的 [`ApiError`] 收敛回既有 IPC 的
//! [`CommandError`] 字符串外观（[`error`]）。
//!
//! ## 明确**不做**什么（开发计划 :84 回退条款）
//!
//! :84 逐字要求：
//!
//! > Tauri adapter 可仍调用原用例实现，但**不得把新 owner 和权限语义伪映射为旧共享
//! > session**。新 frontend bridge 在未注入时**明确报错**。
//!
//! 本模块据此执行三条硬约束：
//!
//! 1. **不把 owner / 权限语义塞进旧共享槽位。**
//!    `AppState::session_transactions` 是 `HashMap<String, TransactionHandle>`，键只有
//!    `dbSessionId`，类型上无法表达 owner；[`handles`] 用 owner 作用域键另起一张登记表，
//!    两者并存但互不写入，迁移缺口在 [`handles`] 的文档与测试里写明（CM-73 "不改"）。
//! 2. **不为"接口齐全"补空壳。**
//!    [`ConnectionUseCases`](datazen_application::sessions::ConnectionUseCases) 的 13 个方法
//!    本 track **不实现**。理由不是省事：每一个方法都要求旧实现**并不具备**的字段
//!    （`config_revision` / `credential_revision` / `runtime_epoch` / 幂等记录 /
//!    `attachment_token` / `stream_id`）。硬接就只能凭空造 revision 与 epoch，
//!    那正是 :84 禁止的伪映射（概要 §2.6 也要求迁移期丢弃不透明 revision）。详见
//!    [`adapter`] 的模块文档。
//! 3. **未注入即报错。**
//!    [`bridge`] 是给前端桥预留的注入口；未注入时 [`bridge::require_bridge`] 返回带固定
//!    文案的错误，**不静默回退到 Tauri 直连**（概要 §7.4）。后端侧对应的显式错误入口是
//!    [`adapter::PlatformAdapter::require_application_services`]。
//!
//! ## §6.2 组装顺序（本 track 覆盖到的部分）
//!
//! §6.2 的顺序是 `1 DriverRegistry → 2 Store → 3 repositories → 4 环境适配 → 5 运行时适配
//! → 6 Runtime → 7 ApplicationServices → 8 Tauri command 窄适配 → 9 state.manage`。
//!
//! 本 track 在第 8 步落位：identity / owner / error / bridge / handle registry 就绪，
//! 第 6 步 `Runtime` 与第 7 步 `ApplicationServices::new` 的 `sessions` 实参缺席，
//! 因此 [`adapter::PlatformAdapter::application_services`] 恒为 `None`，
//! 取用方必须走 [`adapter::PlatformAdapter::require_application_services`] 拿到显式错误。

pub mod adapter;
pub mod bridge;
pub mod error;
pub mod handles;
pub mod identity;
pub mod ipc;

pub use adapter::{PlatformAdapter, PlatformEntry, ASSEMBLY_STATE};
pub use bridge::{BridgeInjection, FrontendBridge};
pub use error::into_command_error;
pub use handles::{HandleKey, SessionHandleRegistry, SessionScopedHandle};
pub use identity::{
    local_principal, new_client_instance_id, DesktopIdentity, OwnerIntent, LOCAL_ORGANIZATION_ID,
};
pub use ipc::{IpcCallScope, IPC_SESSION_PURPOSE};

/// 结构性测试的公共取样器：抽掉**注释**与 `#[cfg(test)]` 之后的部分。
///
/// 必须去掉注释，否则本模块文档里对 :84 约束的逐条说明会把扫描测试自己判成违规
/// —— 文档写「不碰 `session_transactions`」也含这三个词。只去注释、不动字符串字面量：
/// 后者若被用来藏代码，那本身是可读性问题，不该靠这个工具静默放过。
#[cfg(test)]
pub(crate) fn production_source(file_source: &str) -> String {
    let head = file_source.split("#[cfg(test)]").next().unwrap_or_default();
    let mut out = String::new();
    let mut inside_block_comment = false;
    for line in head.lines() {
        let trimmed = line.trim_start();
        if inside_block_comment {
            if let Some(end) = trimmed.find("*/") {
                inside_block_comment = false;
                out.push_str(&trimmed[end + 2..]);
                out.push('\n');
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        if let Some(start) = line.find("/*") {
            let rest = &line[start + 2..];
            match rest.find("*/") {
                Some(end) => out.push_str(&rest[end + 2..]),
                None => inside_block_comment = true,
            }
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 取出源码里每个 `fn` 的**参数列表**文本（跨行签名也能取全）。
///
/// 用于断言"某类信息只能被组装，不能被当作参数接收"。
#[cfg(test)]
pub(crate) fn fn_parameter_lists(code: &str) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut lists = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = code[cursor..].find("fn ") {
        let start = cursor + offset;
        let Some(paren) = code[start..].find('(') else {
            break;
        };
        let open = start + paren;
        let mut depth = 0usize;
        let mut close = None;
        for (index, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        lists.push(code[open + 1..close].replace('\n', " "));
        cursor = close + 1;
    }
    lists
}
