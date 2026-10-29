pub mod adapter;
pub mod artifact_export;
pub mod connections;
mod diagnostics;
pub mod error;
pub mod event_sink;
pub mod execution_identity;
pub mod feature_ports;
pub mod handles;
pub mod history;
pub mod identity;
pub mod ipc;
pub mod known_hosts;
pub mod live_source;
pub mod policy;
pub mod repositories;
pub mod resource_budget;
pub mod scope;
pub mod session_backend;
pub mod session_commands;
pub mod settings;
mod target_metadata;

/// 宿主侧的**组织作用域形态中立闸门**：生产代码里不得直读桌面的安装级组织常量。
///
/// 随 `--lib` 跑（因而进 CI），自带 kill test 与自证。理由与判据见该文件。
#[cfg(test)]
mod scope_gate;

pub use adapter::{AssemblyState, PlatformAdapter, PlatformEntry};
pub use error::into_command_error;
pub use handles::{HandleKey, SessionHandleRegistry, SessionScopedHandle};
pub use identity::{
    local_principal, new_client_instance_id, DesktopIdentity, OwnerIntent, LOCAL_ORGANIZATION_ID,
};
pub use ipc::{IpcCallScope, IPC_SESSION_PURPOSE};
pub use live_source::DesktopLiveConnectionSource;
pub use scope::{is_local_scope, local_organization_scope};
pub(crate) use target_metadata::into_execution_target;

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

/// 宿主侧的**组织 id 字面量闸门**：生产代码里不得把组织 id 写死成字符串字面量。
///
/// 随 `--lib` 跑（因而进 CI），自带 kill test、不误伤用例与自证。理由与判据见该文件。
#[cfg(test)]
mod org_literal_gate;
