//! `MonitorGrantReader`：监控授权单（monitor grant）的**只读**窄端口。
//!
//! ## 它替代什么
//!
//! 桌面侧有**两处**为了读一份授权单而持着整个 `Arc<DesktopRepository>`：
//!
//! * `src-tauri/src/monitor/grants.rs` 的 `current(store, ctx)`：读授权单、校验
//!   有效期与「是否已被新版取代」，并给出它被授权触达的目标；
//! * `src-tauri/src/platform/execution_identity.rs` 的 `MonitorDelegationScope`：
//!   持 `Arc<DesktopRepository>` **只为**从授权单里取那个被授权的目标。
//!
//! 两条路径读的是**同一份事实**，因此收成一个端口。
//!
//! ## 与宿主侧 `DelegationScopeBinding` 的关系
//!
//! 桌面宿主有一个 `DelegationScopeBinding` trait（`src-tauri/src/platform/
//! execution_identity.rs`），形状是「给一份委托，答一个被授权的 `ExecutionTarget`」。
//! 它住在**宿主侧**，不在本包——所以本包不能直接复用那个 trait（那会把宿主形态
//! 引进契约层，违反 F-04）。本包的 [`MonitorGrantReader::authorized_target`] 与它
//! **同形**。
//!
//! **因此集成时应收敛为一条**：宿主那条 trait 改为适配本端口（或直接删掉、改用端口），
//! 否则全仓会留下两个同义形状而没人知道它们本该是同一个。
//!
//! ## 词汇表
//!
//! `MonitorGrantScope`：授权单登记的一条作用域（目标 + 目标所属连接的配置版本 +
//! 授权的命令）。授权单**本体**的签发与校验留在
//! `datazen_modules::monitor::domain::grant`，端口层只暴露「这次委托允许触达哪些目标」这一条事实。
//!
//! ## 边界
//!
//! * **只读**，按 `RequestContext` 限定；失败一律 `Result<_, PortError>`。
//! * 端口**不重试、不鉴权、不缓存**。授权单缺失 / 过期 / 已撤销 / 已被新版取代
//!   都是**事实**（`Ok(None)`），不是基础设施故障。

use async_trait::async_trait;

use crate::context::{DelegationRef, RequestContext};
use crate::error::PortError;
use crate::id::ConfigRevision;
use crate::target::ExecutionTarget;

/// 监控授权单登记的一条作用域。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorGrantScope {
    /// 授权单登记的执行目标（连接 + 命名空间）。
    pub target: ExecutionTarget,
    /// 目标所属连接的**配置版本**：配置已变 ⇒ 这一条授权不再适用。
    pub config_revision: ConfigRevision,
    /// 授权的命令（`query` / `execute` …）。
    pub command: String,
}

/// 监控授权单的只读窄端口。
#[async_trait]
pub trait MonitorGrantReader: Send + Sync + 'static {
    /// 当前请求上下文里的 monitor 授权单登记的**全部**作用域。
    ///
    /// 没有委托 id、授权单缺失、已过期、已撤销、已被新版取代 ⇒ `Ok(None)`：
    /// 「没有可用授权」是一个**事实**，不是错误。
    async fn grant_scopes(
        &self,
        ctx: &RequestContext,
    ) -> Result<Option<Vec<MonitorGrantScope>>, PortError>;

    /// 授权单登记的执行目标。形状与宿主侧的委托作用域绑定一致。
    ///
    /// 授权单覆盖**多个连接** ⇒ 失败关闭（`PortError::TokenInvalid`）：一个执行
    /// 身份只对应一个连接（CM-06），「取第一个了事」会越权。
    async fn authorized_target(
        &self,
        ctx: &RequestContext,
        delegation: &DelegationRef,
    ) -> Result<ExecutionTarget, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{
        ClientInstanceId, ConnectionId, DelegationId, OrganizationId, PrincipalId, RequestId,
        Timestamp,
    };
    use crate::target::NamespaceTarget;
    use std::sync::Arc;

    /// 端口必须**对象安全**：能以 `Arc<dyn Trait>` 注入（§4.1 的注入形态）。
    ///
    /// `?Sized` 是承重的：`dyn Trait` 本身不是 `Sized`，少了它这个断言退化成
    /// 「某个 `Sized` 类型满足这几个约束」，与对象安全无关。
    fn assert_object_safe<T: Send + Sync + 'static + ?Sized>() {}

    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            Some(DelegationId::new("del-1")),
        )
    }

    fn delegation() -> DelegationRef {
        DelegationRef::new(
            DelegationId::new("del-1"),
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            ClientInstanceId::new("client-1"),
            "monitor".into(),
            Timestamp::new("2026-01-01T00:00:00Z"),
        )
    }

    fn scope() -> MonitorGrantScope {
        MonitorGrantScope {
            target: ExecutionTarget::new(
                ConnectionId::new("conn-1"),
                NamespaceTarget::database("app"),
            ),
            config_revision: ConfigRevision::new(3),
            command: "query".into(),
        }
    }

    struct StubGrantReader {
        scopes: Option<Vec<MonitorGrantScope>>,
    }

    #[async_trait]
    impl MonitorGrantReader for StubGrantReader {
        async fn grant_scopes(
            &self,
            _ctx: &RequestContext,
        ) -> Result<Option<Vec<MonitorGrantScope>>, PortError> {
            Ok(self.scopes.clone())
        }

        async fn authorized_target(
            &self,
            _ctx: &RequestContext,
            _delegation: &DelegationRef,
        ) -> Result<ExecutionTarget, PortError> {
            self.scopes
                .as_ref()
                .and_then(|scopes| scopes.first())
                .map(|scope| scope.target.clone())
                .ok_or(PortError::TokenInvalid)
        }
    }

    #[tokio::test]
    async fn the_port_is_object_safe_and_injectable_as_an_arc_trait_object() {
        assert_object_safe::<dyn MonitorGrantReader>();
        let reader: Arc<dyn MonitorGrantReader> = Arc::new(StubGrantReader {
            scopes: Some(vec![scope()]),
        });
        let scopes = reader
            .grant_scopes(&context())
            .await
            .expect("读授权单失败必须是端口事实");
        let scopes = scopes.expect("应当有授权单");
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].target.connection_id, ConnectionId::new("conn-1"));
        assert_eq!(scopes[0].config_revision, ConfigRevision::new(3));
        assert_eq!(scopes[0].command, "query");

        let target = reader
            .authorized_target(&context(), &delegation())
            .await
            .expect("授权目标必须可解析");
        assert_eq!(target.connection_id, ConnectionId::new("conn-1"));
    }

    #[tokio::test]
    async fn a_missing_grant_is_a_fact_not_an_error() {
        let reader: Arc<dyn MonitorGrantReader> = Arc::new(StubGrantReader { scopes: None });
        assert!(reader
            .grant_scopes(&context())
            .await
            .expect("读失败")
            .is_none());
        // 没有授权单 ⇒ 授权目标解析失败关闭，不退回「随便给一个目标」。
        let error = reader
            .authorized_target(&context(), &delegation())
            .await
            .expect_err("没有授权单时不得给出目标");
        assert_eq!(error, PortError::TokenInvalid);
    }

    #[test]
    fn the_reader_declares_only_the_two_read_paths() {
        let source = code_only(include_str!("monitor_grant.rs"));
        let declaration = source
            .split("pub trait MonitorGrantReader")
            .nth(1)
            .expect("trait 声明必须存在")
            .split("\n}")
            .next()
            .unwrap_or_default();
        assert_eq!(
            declaration.matches("async fn").count(),
            2,
            "授权单端口只允许两个方法：{declaration}"
        );
        // 签发 / 撤销 / 保存授权单**不**在这个只读端口上。
        for forbidden in ["issue", "revoke", "save", "write"] {
            assert!(
                !declaration.contains(forbidden),
                "只读端口不得出现写方法 `{forbidden}`：{declaration}"
            );
        }
    }

    #[test]
    fn the_scope_shape_carries_the_target_and_its_config_revision() {
        let scope = scope();
        assert_eq!(scope.target.connection_id, ConnectionId::new("conn-1"));
        assert_eq!(scope.config_revision.counter().get(), 3);
        // 作用域里没有秘密材料：只有目标、版本与命令名。
        let debug = format!("{scope:?}");
        assert!(!debug.to_ascii_lowercase().contains("password"));
    }
}
