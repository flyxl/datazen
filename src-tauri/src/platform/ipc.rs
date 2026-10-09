//! 概要 §6.2 第 8 步：Tauri command 窄适配。
//!
//! ## 为什么单独一个模块
//!
//! §6.2 的组装顺序里，第 8 步夹在"`ApplicationServices` 就绪"和"`state.manage`"
//! 之间。它的职责很窄：**把既有 IPC 的调用现场，变成应用服务认得的形状**。
//! 做得越集中，越容易证明它没顺手改了别的。
//!
//! 因此本模块只提供三件事，且刻意都不碰既有命令的签名、入参与返回值：
//!
//! 1. [`begin_ipc_call`] —— 硬失败入口。新能力一律从这里进：身份组装不出来就报错，
//!    不猜主体、不回落。
//! 2. [`observe_ipc_call`] —— 只做日志对账、不改变控制流。**为什么允许它不失败**见
//!    该函数文档：既有命令走的是旧用例实现，开发计划 :84 明确允许，
//!    CM-73 也明确"不改"。让老命令因为读不到桌面用户名而全线报错，是把
//!    "窄适配"做成"宽回归"。
//! 3. [`require_application_services`] —— 需要应用服务时的显式错误入口。
//!    §6.2 第 6/7 步未落地，它今天必然报错（见 [`super::adapter`]）。
//!
//! ## owner 从哪来
//!
//! 既有 IPC 外观**没有** owner 入参，而 :77 要求保留该外观。所以这一层用
//! [`OwnerIntent::ClientSession`]：归属范围是**当前进程实例**（`clientInstanceId`），
//! 由 adapter 组装，不来自请求体，也不来自前端。这与"旧共享 session"是两回事——
//! 后者没有 owner 概念，前者每次调用都能报出归属。
//!
//! **不**做的事：把 owner / 权限信息写进 `AppState::session_transactions`。
//! 那张表的键只有 `dbSessionId`，写进去就是把新语义伪映射成旧共享 session
//! （:84 明令禁止），回归防护见本模块的
//! `no_owner_data_flows_into_the_legacy_shared_slot`。

use std::sync::Arc;

use datazen_application::ApplicationServices;
use datazen_platform_api::context::{OwnerRef, RequestContext};

use super::adapter::CallScope;
use super::error::into_command_error;
use super::identity::OwnerIntent;
use crate::commands::{AppState, CommandError};

/// 既有 IPC 调用使用的 owner 归属用途串。
///
/// 只用于日志与归属展示，**不是**权限判据（概要 §5.1(3)：`clientInstanceId`
/// 不构成授权依据）。
pub const IPC_SESSION_PURPOSE: &str = "tauri-ipc-session";

/// 一次既有 IPC 调用的窄适配结果：应用服务形状的身份 + 归属。
#[derive(Debug)]
pub struct IpcCallScope {
    scope: CallScope,
    owner: OwnerRef,
}

impl IpcCallScope {
    pub fn context(&self) -> &RequestContext {
        self.scope.context()
    }

    /// 本次调用的 `requestId`。UUID，供日志对账。
    pub fn request_id(&self) -> &str {
        self.scope.request_id().as_str()
    }

    pub fn owner(&self) -> &OwnerRef {
        &self.owner
    }

    /// 便于直接塞进 `tracing` span 的字段。
    pub fn principal(&self) -> &str {
        self.scope.context().principal_id.as_str()
    }

    pub fn client_instance(&self) -> &str {
        self.scope.context().client_instance_id.as_str()
    }
}

/// 硬失败入口：组装不出身份就返回明确错误。
///
/// 新接线的能力（例如 owner 作用域的会话句柄）必须走这条：拿不到 `PrincipalId`
/// 就该失败，而不是编一个。
pub fn begin_ipc_call(state: &AppState) -> Result<IpcCallScope, CommandError> {
    let adapter = state.platform_require().map_err(into_command_error)?;
    let (scope, owner) = adapter
        .begin_owned_call(&OwnerIntent::ClientSession {
            purpose: IPC_SESSION_PURPOSE.to_string(),
        })
        .map_err(into_command_error)?;
    Ok(IpcCallScope { scope, owner })
}

/// 只做日志对账的入口，**不改变控制流**。
///
/// 既有 `connect` / `release_connection` / … 走的是旧用例实现，:84 明确允许保留。
/// 把它们改成"读不到桌面用户名就失败"会造成真实回归（无头 CI 里 `USER` 可能为空），
/// 而它们**本来就没有** owner 语义可伪映射——所以这里返回 `None` 只是不写日志字段，
/// 不是"回落到共享 session"。失败原因已在 [`super::adapter::PlatformEntry::launch`]
/// 打过一次 `warn`。
pub fn observe_ipc_call(state: &AppState) -> Option<IpcCallScope> {
    let adapter = state.platform_require().ok()?;
    let (scope, owner) = adapter
        .begin_owned_call(&OwnerIntent::ClientSession {
            purpose: IPC_SESSION_PURPOSE.to_string(),
        })
        .ok()?;
    Some(IpcCallScope { scope, owner })
}

/// 需要应用服务时的唯一入口；今天必然报出可定位错误。
pub fn require_application_services(
    state: &AppState,
) -> Result<Arc<ApplicationServices>, CommandError> {
    state
        .platform_require()
        .map_err(into_command_error)?
        .require_application_services()
        .map_err(into_command_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::ClientInstanceId;
    use serde_json::json;

    /// 既有 IPC 外观的**冻结签名**。
    ///
    /// 开发计划 :77 要求"保留现有 IPC 外观"。这类要求靠人读 diff 是守不住的：
    /// 加一个参数、改一个返回类型，名字和数量都不变，CI 也不会报。
    /// 所以把 14 个 `#[tauri::command]` 与 13 个 `*_impl` 的完整签名钉在这里，
    /// 任何人动外观都会让这个测试变红。
    const FROZEN_SIGNATURES: &[&str] = &[
        "pub async fn get_connections( state: State<'_, AppState>, ) -> Result<Vec<ConnectionConfig>, CommandError>",
        "pub async fn save_connection( state: State<'_, AppState>, config: ConnectionConfig, ) -> Result<(), CommandError>",
        "pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), CommandError>",
        "pub async fn test_connection( state: State<'_, AppState>, config: ConnectionConfig, ) -> Result<ServerInfo, CommandError>",
        "pub async fn connect( state: State<'_, AppState>, connection_id: String, ) -> Result<String, CommandError>",
        "pub async fn connect_dedicated( state: State<'_, AppState>, connection_id: String, database: Option<String>, ) -> Result<String, CommandError>",
        "pub async fn ping_connection( state: State<'_, AppState>, db_session_id: String, ) -> Result<bool, CommandError>",
        "pub async fn release_connection( state: State<'_, AppState>, db_session_id: String, ) -> Result<bool, CommandError>",
        "pub async fn disconnect( state: State<'_, AppState>, db_session_id: String, ) -> Result<(), CommandError>",
        "pub async fn close_database( state: State<'_, AppState>, db_session_id: String, database: String, ) -> Result<bool, CommandError>",
        "pub async fn get_open_databases( state: State<'_, AppState>, db_session_id: String, ) -> Result<Vec<String>, CommandError>",
        "pub async fn get_connection_info( state: State<'_, AppState>, db_session_id: String, ) -> Result<serde_json::Value, CommandError>",
        "pub async fn reorder_connections( state: State<'_, AppState>, ordered_ids: Vec<String>, ) -> Result<(), CommandError>",
        "pub async fn get_available_drivers( state: State<'_, AppState>, ) -> Result<Vec<String>, CommandError>",
        "pub(crate) async fn get_connections_impl( state: &AppState, ) -> Result<Vec<ConnectionConfig>, CommandError>",
        "pub(crate) async fn save_connection_impl( state: &AppState, config: ConnectionConfig, ) -> Result<(), CommandError>",
        "pub(crate) async fn delete_connection_impl( state: &AppState, id: String, ) -> Result<(), CommandError>",
        "pub(crate) async fn test_connection_impl( state: &AppState, config: ConnectionConfig, ) -> Result<ServerInfo, CommandError>",
        "pub(crate) async fn connect_impl( state: &AppState, connection_id: String, ) -> Result<String, CommandError>",
        "pub(crate) async fn connect_dedicated_impl( state: &AppState, connection_id: String, database: Option<String>, ) -> Result<String, CommandError>",
        "pub(crate) async fn ping_connection_impl( state: &AppState, db_session_id: String, ) -> Result<bool, CommandError>",
        "pub(crate) async fn release_connection_impl( state: &AppState, db_session_id: String, ) -> Result<bool, CommandError>",
        "pub(crate) async fn disconnect_impl( state: &AppState, db_session_id: String, ) -> Result<(), CommandError>",
        "pub(crate) async fn close_database_impl( state: &AppState, db_session_id: String, database: String, ) -> Result<bool, CommandError>",
        "pub(crate) async fn get_open_databases_impl( state: &AppState, db_session_id: String, ) -> Result<Vec<String>, CommandError>",
        "pub(crate) async fn get_connection_info_impl( state: &AppState, db_session_id: String, ) -> Result<serde_json::Value, CommandError>",
        "pub(crate) async fn get_available_drivers_impl( state: &AppState, ) -> Result<Vec<String>, CommandError>",
    ];

    const CONNECTION_COMMANDS: &str = include_str!("../commands/connection.rs");

    /// 抽出 `pub async fn` / `pub(crate) async fn` 的完整签名（跨行签名也能取全）。
    fn extract_signatures(code: &str) -> Vec<String> {
        let bytes = code.as_bytes();
        let mut out = Vec::new();
        let mut cursor = 0usize;
        while let Some(offset) = code[cursor..].find("async fn ") {
            let start = cursor + offset;
            // 回退到 `pub` / `pub(crate)` 的行首，保证签名文本从可见性开始。
            let head = code[..start].rfind('\n').map(|nl| nl + 1).unwrap_or(0);
            let Some(open) = code[start..].find('(').map(|p| start + p) else {
                break;
            };
            let mut depth = 0i32;
            let mut opened = false;
            let mut close = None;
            for (index, byte) in bytes.iter().enumerate().skip(open) {
                match byte {
                    b'(' => {
                        depth += 1;
                        opened = true;
                    }
                    b')' => {
                        depth -= 1;
                        if opened && depth == 0 {
                            close = Some(index);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close) = close else { break };
            let params = code[open..=close].replace('\n', " ");
            // 返回类型：从 `)` 之后到 body 的 `{`。
            let mut cursor_j = close + 1;
            let mut ret_end = code.len();
            while cursor_j < code.len() {
                let ch = code.as_bytes()[cursor_j];
                if ch == b'{' {
                    ret_end = cursor_j;
                    break;
                }
                cursor_j += 1;
            }
            let ret = code[close + 1..ret_end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            out.push(format!(
                "{} {}{} {}",
                code[head..start].trim().replace("async fn ", "").trim(),
                code[start..open].trim(),
                params,
                ret
            ));
            cursor = ret_end.max(close + 1);
        }
        out
    }

    fn normalized(code: &str) -> Vec<String> {
        let mut out = extract_signatures(code)
            .into_iter()
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>();
        // 比对签名集合而非源码顺序：函数在文件里挪个位置不是外观变更，
        // 增删改签名才是。
        out.sort();
        out
    }

    #[test]
    fn existing_ipc_facade_signatures_are_frozen() {
        let production = super::super::production_source(CONNECTION_COMMANDS);
        let actual = normalized(&production);
        let mut expected = FROZEN_SIGNATURES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(
            actual.len(),
            expected.len(),
            "connection.rs 的对外函数数量变了：新增或删除命令都会让前端调用面漂移，\
             必须是有意改动并同步更新本冻结表（实际 {actual:#?}）"
        );
        for (want, got) in expected.iter().zip(actual.iter()) {
            assert_eq!(got, want, "既有 IPC 外观被改动；:77 要求保留");
        }
    }

    #[test]
    fn the_command_count_is_exactly_fourteen() {
        // 冻结表也可能被"改到和新代码一致"而失去意义，所以单独钉住命令数量，
        // 并且只数生产代码里的 `#[tauri::command]`。
        let production = super::super::production_source(CONNECTION_COMMANDS);
        assert_eq!(
            production.matches("#[tauri::command]").count(),
            14,
            "connection.rs 对外暴露的 Tauri 命令数量变了"
        );
        assert_eq!(
            production.matches("pub(crate) async fn ").count(),
            13,
            "connection.rs 的 *_impl 数量变了"
        );
    }

    #[test]
    fn narrow_adaptation_never_takes_identity_from_the_request_body() {
        // 身份只能由 adapter 组装。凡是把 organization/principal/client_instance
        // 作为**参数**接收的入口都是伪适配。
        for params in
            super::super::fn_parameter_lists(&super::super::production_source(CONNECTION_COMMANDS))
        {
            for forbidden in [
                "RequestContext",
                "OwnerRef",
                "DesktopIdentity",
                "PrincipalId",
            ] {
                assert!(
                    !params.contains(forbidden),
                    "connection.rs 出现了接收身份的参数 `{params}`，身份必须由 adapter 组装"
                );
            }
        }
        for params in super::super::fn_parameter_lists(&super::super::production_source(
            include_str!("ipc.rs"),
        )) {
            assert!(
                !params.contains("RequestContext") && !params.contains("OwnerRef"),
                "ipc.rs 的窄适配入口接收了调用方自带的身份：{params}"
            );
        }
    }

    #[test]
    fn no_owner_data_flows_into_the_legacy_shared_slot() {
        // :84 第一条：不把新 owner / 权限语义伪映射为旧共享 session。
        let files = [
            CONNECTION_COMMANDS,
            include_str!("adapter.rs"),
            include_str!("ipc.rs"),
            include_str!("identity.rs"),
            include_str!("handles.rs"),
            include_str!("bridge.rs"),
            include_str!("../commands/mod.rs"),
            include_str!("../bootstrap/app_state.rs"),
        ];
        let owner_words = [
            "OwnerRef",
            "OwnerIntent",
            "RequestContext",
            "DesktopIdentity",
        ];
        for file in files {
            let code = super::super::production_source(file);
            for line in code.lines() {
                if !line.contains("session_transactions") {
                    continue;
                }
                for word in owner_words {
                    assert!(
                        !line.contains(word),
                        "旧共享槽位上出现了 owner 语义 `{word}`：{}",
                        line.trim()
                    );
                }
            }
        }
    }

    #[test]
    fn observe_never_fails_the_command_and_begin_does() {
        // 无 `AppState` 时无法构造真实调用，这里验证两条入口的**契约形状**：
        // `begin` 返回 Result（可失败），`observe` 返回 Option（不改变控制流）。
        let begin: fn(&AppState) -> Result<IpcCallScope, CommandError> = begin_ipc_call;
        let observe: fn(&AppState) -> Option<IpcCallScope> = observe_ipc_call;
        let _: fn(&AppState) -> Result<Arc<ApplicationServices>, CommandError> =
            require_application_services;
        // 上面只是引用，证明签名稳定；不实际调用（需要完整的 AppState）。
        let _ = (begin, observe);
        assert_eq!(IPC_SESSION_PURPOSE, "tauri-ipc-session");
    }

    #[test]
    fn owner_intent_for_the_ipc_surface_is_assembled_not_supplied() {
        // 归属用途串是常量，不是从入参取的：命令外观里根本没有这一项。
        assert!(!FROZEN_SIGNATURES
            .iter()
            .any(|s| s.contains("purpose") || s.contains("owner")));
        // 且组装出的 owner 一定是本进程实例。
        let identity = super::super::identity::DesktopIdentity::new_launch(
            super::super::LOCAL_ORGANIZATION_ID,
            "alice",
            "client-1",
        )
        .expect("固定输入应当组装成功");
        let owner = OwnerIntent::ClientSession {
            purpose: IPC_SESSION_PURPOSE.to_string(),
        }
        .admit(&identity)
        .expect("本进程实例的 client session owner 必须通过");
        match owner {
            OwnerRef::ClientSession {
                client_instance_id,
                purpose,
            } => {
                assert_eq!(client_instance_id, ClientInstanceId::new("client-1"));
                assert_eq!(purpose, IPC_SESSION_PURPOSE);
            }
            other => panic!("必须组装为 ClientSession owner，实际 {other:?}"),
        }
    }

    #[test]
    fn every_request_id_is_distinct_and_json_safe() {
        // requestId 会进结构化日志，必须是可直接序列化的 UUID 字符串。
        let identity = super::super::identity::DesktopIdentity::new_launch(
            super::super::LOCAL_ORGANIZATION_ID,
            "alice",
            "client-1",
        )
        .expect("固定输入应当组装成功");
        let first = identity.next_request_context();
        let second = identity.next_request_context();
        assert_ne!(first.request_id, second.request_id);
        let rendered = json!(first.request_id.as_str());
        assert!(rendered.as_str().expect("应当可序列化").len() >= 32);
    }
}
