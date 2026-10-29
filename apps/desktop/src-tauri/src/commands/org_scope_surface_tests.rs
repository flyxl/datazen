//! 组织作用域**不得**出现在 IPC 载荷里（验证轨道 D 的边界守卫）。
//!
//! 组织身份只来自 adapter 组装的请求上下文（INV-01）：命令参数里一旦出现
//! `OrganizationScope` / `OrganizationId`，就意味着「调用方可以指定组织」——
//! 请求体就能覆盖身份，租户边界随之失效。
//!
//! 这条守卫同时锁住 P7：组织作用域改造**没有**改动任何 IPC 命令的签名。
//! 以后谁想把 scope 加进命令参数，这条用例会先红。

use std::path::{Path, PathBuf};

/// 宿主源码根（`src-tauri/`）。
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 宿主源码目录（`src-tauri/src/`）。
fn source_root() -> PathBuf {
    crate_root().join("src")
}

/// 递归收集所有 `.rs` 文件。
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 取出源文件里每个 `#[tauri::command]` 命令函数的**参数列表**。
///
/// 返回 `(函数名, 参数列表文本)`。参数列表跨行也能取全（按括号配平）。
fn command_params(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = source[cursor..].find("#[tauri::command]") {
        let start = cursor + offset;
        let tail = &source[start..];
        let Some(fm) = tail.find("fn ") else {
            break;
        };
        let name_end = tail[fm + 3..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map(|len| fm + 3 + len)
            .unwrap_or(tail.len());
        let name = tail[fm + 3..name_end].to_string();
        let Some(open) = tail[name_end..].find('(') else {
            break;
        };
        let open = name_end + open;
        let mut depth = 0usize;
        let mut close = None;
        for (index, ch) in tail[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else {
            break;
        };
        out.push((name, tail[open..=close].to_string()));
        cursor = start + close;
    }
    out
}

/// 身份类类型名：出现在命令参数里就意味着请求体可以覆盖身份。
const IDENTITY_TYPES: [&str; 4] = [
    "OrganizationScope",
    "OrganizationId",
    "RequestContext",
    "DesktopIdentity",
];

/// IPC 命令的参数里不得出现任何身份类类型。
#[test]
fn no_ipc_command_takes_an_identity_parameter() {
    let root = source_root();
    let mut scanned = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for path in rust_sources(&root) {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (name, params) in command_params(&source) {
            scanned += 1;
            for identity in IDENTITY_TYPES {
                if params.contains(identity) {
                    offenders.push(format!(
                        "{}::{name} 的参数里有 {identity}：{params}",
                        path.display()
                    ));
                }
            }
        }
    }

    assert!(
        scanned >= 250,
        "命令扫描取样器应当覆盖全部 IPC 命令，实际只取到 {scanned} 个"
    );
    assert!(
        offenders.is_empty(),
        "组织作用域 / 身份不得进入 IPC 载荷：\n{}",
        offenders.join("\n")
    );
}

/// 仪表板与 workflow 命令的命令签名里没有作用域参数（轨道 D 改造的边界）。
///
/// 这两条命令现在从 `AppState` 内部取作用域；一旦有人为了「少查一次」把
/// scope 提成命令参数，这条用例会先红。
#[test]
fn the_dashboard_and_workflow_commands_take_no_scope_parameter() {
    let root = source_root();
    let mut checked = 0usize;
    let mut offenders: Vec<String> = Vec::new();

    for relative in ["dashboard.rs", "workflow.rs"] {
        let path = root.join("commands").join(relative);
        let source = std::fs::read_to_string(&path).expect("命令源码可读");
        for (name, params) in command_params(&source) {
            checked += 1;
            if params.contains("OrganizationScope") || params.contains("OrganizationId") {
                offenders.push(format!("commands/{relative}::{name}：{params}"));
            }
        }
    }

    assert!(checked > 0, "应当取到仪表板 / workflow 命令");
    assert!(
        offenders.is_empty(),
        "仪表板 / workflow 命令不得把作用域做成 IPC 参数：\n{}",
        offenders.join("\n")
    );
}
