//! modules application 面的 server 侧装配（`datazen/mcp` feature 门控）。
//!
//! 主线点亮方式（WR-F2 接线清单）：`server/Cargo.toml`
//! `[features] mcp = ["datazen/mcp"]`，然后 bootstrap 构造
//! [`HttpMcpState`]（backend = [`ModulesBackend`]，auth = L2 登录会话适配器）
//! 并把 `mcp_router` merge 进 router。
//!
//! ## resolve_call 的租约来源（server 执行域最小装配说明）
//!
//! 桌面形态的 `DataZenMcpServer::resolve_call` 走 `AppState.platform_require()
//! .runtime()`（每调用现取，不缓存）。server 形态执行域现状：bootstrap 尚未
//! 装配 `RuntimeConnectionUseCases`，故本模块定义 [`RuntimeLeaseSource`] 端口，
//! 主线按 `modules::execution::application::RuntimeConnectionUseCases::new(
//! profiles: Arc<dyn ProfileRepository>, policy: Arc<dyn PolicyService>,
//! backend: Arc<dyn SessionBackend>)` 装配（profiles 取 `storage/profiles.rs`
//! 仓储适配器、policy 取 `policy/` 的 PolicyService、backend 为执行域会话
//! 后端），包成每调用现取的 lease source——runtime 可能被吊销，端口语义
//! 禁止缓存。
//!
//! 服务端身份 = L2 认证链登录会话：`CallIdentity`（来自
//! `http::SessionAuthenticator`）逐请求落 `RequestContext`（组织隔离沿
//! ctx），`clientInstanceId` 是本 server 实例后端生成的 id（§4.1）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;

use crate::mcp::http::{CallIdentity, HttpMcpState, McpBackend, SessionAuthenticator};
use crate::mcp::wire::ToolCallError;

use datazen::application::identity_policy::IdentityPolicy;
use datazen::execution::application::RuntimeConnectionUseCases;
use datazen::mcp::application::session::McpSessionRegistry;
use datazen::mcp::application::session_tools::{
    self, OwnerStamp, StatefulCall, StatefulSessionHost,
};
use datazen::mcp::domain::allowlist;
use datazen::mcp::domain::permission::McpPermissionMode;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::id::ClientInstanceId;

/// Owner purpose recorded for stateful MCP sessions（与桌面
/// `mcp::server::MCP_SESSION_PURPOSE` 逐字一致；仅展示，从不作为授权输入）。
pub const MCP_SESSION_PURPOSE: &str = "mcp-stateful-session";

/// runtime 租约端口：每次调用现取当前 runtime（可能被吊销，禁止缓存）。
#[async_trait::async_trait]
pub trait RuntimeLeaseSource: Send + Sync {
    async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String>;
}

/// server 形态的 `StatefulSessionHost` 内核：租约解析 + 归属裁决 +
/// cleanup worker 调度钩子（admission/编排语义在 modules session_tools）。
pub struct HttpStatefulSessionHost {
    lease_source: Arc<dyn RuntimeLeaseSource>,
    /// 本 server 实例的后端生成 clientInstanceId（§4.1）。
    client_instance_id: String,
    registry: Arc<McpSessionRegistry>,
    worker_started: AtomicBool,
}

impl HttpStatefulSessionHost {
    pub fn new(
        lease_source: Arc<dyn RuntimeLeaseSource>,
        client_instance_id: String,
        registry: Arc<McpSessionRegistry>,
    ) -> Self {
        Self {
            lease_source,
            client_instance_id,
            registry,
            worker_started: AtomicBool::new(false),
        }
    }

    pub fn owner_stamp(&self) -> OwnerStamp {
        OwnerStamp {
            client_instance_id: self.client_instance_id.clone(),
            purpose: MCP_SESSION_PURPOSE.to_owned(),
        }
    }

    fn request_context(&self, identity: &CallIdentity) -> RequestContext {
        RequestContext::new(
            identity.organization_id.clone().into(),
            identity.principal_id.clone().into(),
            None,
            ClientInstanceId::new(self.client_instance_id.clone()),
            identity.request_id.clone().into(),
            None,
        )
    }

    /// TTL 过期回收的最小调度：purge 判据在 modules registry，此处保活
    /// 节拍；数据库侧关闭沿用 desktop `session_cleanup` 同款路径（主线
    /// 接线项，见 WR-F2 报告）。
    fn start_cleanup_worker(&self) {
        if self.worker_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let registry = Arc::downgrade(&self.registry);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let Some(registry) = registry.upgrade() else {
                    break; // host 已随 server 关闭；节拍随之结束。
                };
                registry.purge_expired(Instant::now());
                registry.wake.notify_one();
            }
        });
    }
}

/// 一次调用内生效的宿主视图：把逐请求身份（L2 登录会话）与共享内核绑定，
/// 避免共享槽位在并发调用间串号。
struct PerCallHost {
    inner: Arc<HttpStatefulSessionHost>,
    identity: CallIdentity,
}

#[async_trait::async_trait]
impl StatefulSessionHost for PerCallHost {
    async fn resolve_call(&self) -> Result<StatefulCall, String> {
        let runtime = self.inner.lease_source.resolve().await?;
        Ok(StatefulCall {
            runtime,
            context: self.inner.request_context(&self.identity),
        })
    }

    fn authorize_owner(&self, context: &RequestContext, owner: &OwnerRef) -> Result<(), String> {
        // MCP 从不自带 ownership allow 规则：形态身份策略裁决（与桌面
        // server.rs 同一入口，委托恒 None）。
        datazen::application::identity_policy::IdentityPolicy::check_owner(context, owner, None)
            .map_err(|error| error.to_string())
    }

    fn ensure_cleanup_worker(&self) {
        self.inner.start_cleanup_worker();
    }
}

/// modules application 面的 server 侧工具分发。
pub struct ModulesBackend {
    host: Arc<HttpStatefulSessionHost>,
    registry: Arc<McpSessionRegistry>,
    /// 空表即全拒绝的安全默认（allowlist 语义与桌面一致）。
    allowed_connection_ids: Vec<String>,
    permission_mode: McpPermissionMode,
}

impl ModulesBackend {
    pub fn new(
        lease_source: Arc<dyn RuntimeLeaseSource>,
        allowed_connection_ids: Vec<String>,
        permission_mode: McpPermissionMode,
    ) -> Self {
        let registry = Arc::new(McpSessionRegistry::new());
        let client_instance_id = crate::mcp::http::new_client_instance_id();
        Self {
            host: Arc::new(HttpStatefulSessionHost::new(
                lease_source,
                client_instance_id,
                registry.clone(),
            )),
            registry,
            allowed_connection_ids,
            permission_mode,
        }
    }

    /// 传输断开/会话终止路径：断连置位（后续有状态调用报 SessionLost）
    /// 并触发回收节拍。
    pub fn release_sessions(&self) {
        self.registry.disconnected.store(true, Ordering::Release);
        self.host.start_cleanup_worker();
    }

    fn ensure_allowed(&self, tool: &str, connection_id: &str) -> Result<(), ToolCallError> {
        allowlist::ensure_connection_allowed(connection_id, &self.allowed_connection_ids)
            .map_err(|e| ToolCallError::WithHelp {
                tool: tool.to_owned(),
                reason: e.to_string(),
            })
    }
}

#[async_trait::async_trait]
impl McpBackend for ModulesBackend {
    async fn call_tool(
        &self,
        identity: &CallIdentity,
        name: &str,
        arguments: Value,
    ) -> Result<String, ToolCallError> {
        // 连接白名单：与桌面 handler 同点位（arguments.connection_id）。
        if let Some(connection_id) = arguments.get("connection_id").and_then(Value::as_str) {
            self.ensure_allowed(name, connection_id)?;
        }
        let stamp = self.host.owner_stamp();
        let host = PerCallHost {
            inner: self.host.clone(),
            identity: identity.clone(),
        };
        match name {
            "open_session" => {
                let connection_id = require_str(&arguments, "connection_id", name)?;
                let database = require_str(&arguments, "database", name)?;
                let schema = optional_str(&arguments, "schema");
                let ttl_ms = arguments.get("ttl_ms").and_then(Value::as_u64);
                session_tools::open_session(
                    Arc::new(host) as Arc<dyn StatefulSessionHost>,
                    self.registry.clone(),
                    &stamp,
                    connection_id,
                    Some(database.to_string()),
                    schema,
                    ttl_ms,
                )
                .await
                .map_err(|e| map_session_tool_error(name, e))
            }
            "use_session" => {
                let reference = require_str(&arguments, "client_session_ref", name)?;
                let command = require_str(&arguments, "command", name)?;
                let input = arguments.get("input").cloned().unwrap_or(Value::Null);
                let timeout_ms = arguments.get("timeout_ms").and_then(Value::as_u64);
                session_tools::use_session(
                    &host,
                    &self.registry,
                    self.permission_mode,
                    &stamp,
                    reference,
                    command,
                    input,
                    timeout_ms,
                )
                .await
                .map_err(|e| map_session_tool_error(name, e))
            }
            "close_session" => {
                let reference = require_str(&arguments, "client_session_ref", name)?;
                let mode = optional_str(&arguments, "mode");
                session_tools::close_session(
                    &host,
                    &self.registry,
                    &stamp,
                    reference,
                    mode.as_deref(),
                )
                .await
                .map_err(|e| map_session_tool_error(name, e))
            }
            // 无状态工具（query/list_*/run_workflow/...）主线装配：把
            // `storage/` 仓储适配成 modules tools.rs 的端口
            // （RedactedConnectionSource / WorkflowCatalog）后逐工具接上；
            // 本切片只落传输与有状态面（验收 golden 只覆盖定义与 wire）。
            other => Err(ToolCallError::internal(format!(
                "tool '{other}' is not wired in the server form yet (mainline assembly pending)"
            ))),
        }
    }
}

fn map_session_tool_error(tool: &str, error: session_tools::SessionToolError) -> ToolCallError {
    match error {
        session_tools::SessionToolError::WithHelp(reason) => ToolCallError::WithHelp {
            tool: tool.to_owned(),
            reason,
        },
        session_tools::SessionToolError::Plain(reason) => ToolCallError::Plain(reason),
        session_tools::SessionToolError::Internal(reason) => ToolCallError::Internal(reason),
    }
}

fn require_str<'a>(args: &'a Value, key: &str, tool: &str) -> Result<&'a str, ToolCallError> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| {
        ToolCallError::WithHelp {
            tool: tool.to_owned(),
            reason: format!("missing required input property: {key}"),
        }
    })
}

fn optional_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// bootstrap 装配入口（主线调用）：登录会话认证器 + 租约来源 → 传输态。
pub fn build_http_state(
    auth: Arc<dyn SessionAuthenticator>,
    lease_source: Arc<dyn RuntimeLeaseSource>,
    allowed_connection_ids: Vec<String>,
    permission_mode: McpPermissionMode,
) -> HttpMcpState {
    HttpMcpState::new(
        Arc::new(ModulesBackend::new(
            lease_source,
            allowed_connection_ids,
            permission_mode,
        )),
        auth,
    )
}
