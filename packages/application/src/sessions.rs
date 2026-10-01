//! 连接用例面（连接 §4.1 的 `ConnectionService`）。
//!
//! ## 这是**契约**，不是实现
//!
//! 依赖图里 `APP → PA, DAPI`（**没有** `APP → RT`），因此 application 无法直接编排
//! 运行时 actor、预算、ArtifactStore 这些实现。本文件固定的是：
//!
//! * 方法集合与顺序语义（§7.1–§7.7）；
//! * 身份只从 `&RequestContext` 来（INV-01，第一个参数恒定）；
//! * 失败一律是 [`ApiError`]，端口层的 [`PortError`](datazen_platform_api::error::PortError)
//!   由组装层映射。
//!
//! 具体编排（谁开 actor、谁记账、谁落幂等记录）在**组装层**按 §6.2 的顺序用
//! `Arc<dyn …>` 实现本 trait 注入。桌面与团队形态共用同一套签名，只有实现不同。
//!
//! ## 不可隐含的顺序约束（§7）
////!
//! * `open_session`：先写注册表与幂等记录，**再**返回 `OpenSessionReceipt`。
//! * `execute_in_session`：先原子登记幂等键与 `executionId`，**再**返回回执。
//! * `execute_at_target`：每次调用使用**独立固定 Lease**，不复用任何会话句柄。
//! * `close_session`：幂等查墓碑；事务处于 Active/Aborted/Unknown 时先要
//!   `requireNoTransaction`，否则 `TransactionResolutionRequired`。
//! * `cancel_execution`：先验证内部绑定，再返回
//!   `CancelReceipt{disposition: requested, state: cancelRequested}`。
//! * `subscribe_events`：**永不隐式取消**执行（§7.7）。

use async_trait::async_trait;

use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::event::EventSequence;
use datazen_platform_api::dto::execution::{ExecutionReceipt, ExecutionView};
use datazen_platform_api::dto::idempotency::{IdempotentOperation, SubmissionToken};
use datazen_platform_api::dto::session::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, OpenSessionReceipt, SessionHandle,
    SessionView,
};
use datazen_platform_api::id::{ExecutionId, StreamId};
use datazen_platform_api::ports::event::EventSubscription;

use crate::dto::profile::ProfileView;
use crate::dto::requests::{
    AttachmentRequest, CloseSessionRequest, ExecuteAtTargetRequest, ExecuteInSessionRequest,
    OpenSessionRequest, SetSessionContextRequest,
};
use crate::error::ApiError;

/// 连接用例面。实现由组装层注入（§6.2 第 7 步之后）。
#[async_trait]
pub trait ConnectionUseCases: Send + Sync + 'static {
    /// 连接配置列表。返回的 `ProfileView` **不含**任何秘密材料（§4.1）。
    async fn list_connections(&self, ctx: &RequestContext) -> Result<Vec<ProfileView>, ApiError>;

    /// 签发幂等提交令牌。
    ///
    /// `createProfile` 是配置写入，**不绑定会话与 epoch**；其余运行时会话操作绑定
    /// `handle` 所属的 owner `runtimeEpoch`（§13.1）。
    async fn issue_submission_token(
        &self,
        ctx: &RequestContext,
        operation: IdempotentOperation,
        handle: Option<&SessionHandle>,
    ) -> Result<SubmissionToken, ApiError>;

    /// §7.1：先写注册表与幂等记录，再返回回执。新会话 `contextRevision = 0`、
    /// `observedContext = unknown`。
    async fn open_session(
        &self,
        ctx: &RequestContext,
        request: OpenSessionRequest,
    ) -> Result<OpenSessionReceipt, ApiError>;

    /// 附着一个已有会话。`attachmentToken` 不匹配 ⇒ `ContextConflict`。
    async fn attach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError>;

    /// 分离附着。**不**关闭会话：分离只解除这个客户端的视图。
    async fn detach_session(
        &self,
        ctx: &RequestContext,
        request: AttachmentRequest,
    ) -> Result<SessionView, ApiError>;

    /// 读会话视图。客户端 `dbSessionId` 与 `runtimeEpoch` 必须**完全一致**（§6.3）。
    async fn get_session(
        &self,
        ctx: &RequestContext,
        handle: SessionHandle,
    ) -> Result<SessionView, ApiError>;

    /// §7.2：原子登记幂等键与 `executionId` 后再返回回执。
    /// 同一 session 已有普通执行 ⇒ 排队，不是 `ResourceBusy`（INV-03）。
    async fn execute_in_session(
        &self,
        ctx: &RequestContext,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, ApiError>;

    /// §7.3：**永不**回填缺失层级（§4.3）。每次调用使用独立固定 Lease。
    async fn execute_at_target(
        &self,
        ctx: &RequestContext,
        request: ExecuteAtTargetRequest,
    ) -> Result<ExecutionReceipt, ApiError>;

    /// §7.4：两阶段候选 → 服务端已提交替换，带 commit barrier。
    /// `expected_context_revision` 不匹配 ⇒ `ContextConflict`。
    async fn set_session_context(
        &self,
        ctx: &RequestContext,
        request: SetSessionContextRequest,
    ) -> Result<ContextChangeReceipt, ApiError>;

    /// §7.5：幂等查墓碑；Active/Aborted/Unknown 事务 ⇒ `TransactionResolutionRequired`。
    async fn close_session(
        &self,
        ctx: &RequestContext,
        request: CloseSessionRequest,
    ) -> Result<CloseReceipt, ApiError>;

    /// 执行视图。已建立执行记录后，终态失败是
    /// `ExecutionState::Failed` + `ExecutionErrorCode`，**不是** `ApiError`。
    async fn get_execution(
        &self,
        ctx: &RequestContext,
        execution_id: ExecutionId,
    ) -> Result<ExecutionView, ApiError>;

    /// §7.6：先验证内部绑定，再回 `disposition: requested`。
    async fn cancel_execution(
        &self,
        ctx: &RequestContext,
        execution_id: ExecutionId,
    ) -> Result<CancelReceipt, ApiError>;

    /// §7.7：订阅**永不**隐式取消执行。`after_sequence` 是重连补偿位点，不落盘。
    async fn subscribe_events(
        &self,
        ctx: &RequestContext,
        stream_id: StreamId,
        after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, ApiError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 剔除测试模块与整行注释后的源码，用于结构性断言。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn trait_code() -> String {
        let source = include_str!("sessions.rs");
        let collapsed = code_only(source.split("#[cfg(test)]").next().unwrap_or_default())
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        collapsed
    }

    #[test]
    fn every_use_case_takes_the_request_context_first() {
        // 断言前先折叠空白，因此这个检查不受 rustfmt 换行影响。
        let code = trait_code();
        for method in [
            "list_connections",
            "open_session",
            "execute_in_session",
            "execute_at_target",
            "set_session_context",
            "close_session",
            "get_execution",
            "cancel_execution",
            "subscribe_events",
            "attach_session",
            "detach_session",
            "get_session",
            "issue_submission_token",
        ] {
            // rustfmt 既可能把参数列表压成一行，也可能拆开；折叠空白后两种形态
            // 只差 `(` 后的一个空格，因此两种都接受。要断言的是**顺序**：
            // `&self` 之后的第一个参数恒为 `ctx: &RequestContext`。
            let wrapped = format!("async fn {method}( &self, ctx: &RequestContext");
            let inline = format!("async fn {method}(&self, ctx: &RequestContext");
            assert!(
                code.contains(&wrapped) || code.contains(&inline),
                "签名形态不符 §5.1(1)：{wrapped}"
            );
        }
    }

    #[test]
    fn the_use_case_surface_has_no_panic_path() {
        let code = trait_code();
        assert!(!code.contains(".unwrap()"), "生产路径不得 unwrap");
        assert!(!code.contains(".expect("), "生产路径不得 expect");
    }

    #[test]
    fn no_use_case_takes_an_identity_argument_from_the_caller() {
        let code = trait_code();
        // INV-01：组织与主体不得作为请求体参数传入，只能经 RequestContext。
        for forbidden in [
            "organization_id: OrganizationId",
            "principal_id: PrincipalId",
            "subject: AuthorizationSubject",
        ] {
            assert!(
                !code.contains(forbidden),
                "身份不得来自调用方参数：{forbidden}"
            );
        }
    }

    #[test]
    fn the_trait_is_object_safe_and_injectable() {
        // 组装层注入 `Arc<dyn ConnectionUseCases>`；对象安全是 §6.2 第 7 步的前提。
        fn assert_object_safe(_: std::sync::Arc<dyn ConnectionUseCases>) {}
        let _ = assert_object_safe;
        let code = trait_code();
        assert!(code.contains("#[async_trait]"));
        assert!(code.contains("pub trait ConnectionUseCases: Send + Sync + 'static"));
        // async_trait 默认产出 Box<dyn Future>，方法必须可 Sized 且不含关联类型。
        assert!(!code.contains("type "), "不得引入关联类型，否则无法 dyn 化");
        assert!(
            !code.contains("impl Trait"),
            "不得用 impl Trait 返回，否则无法 dyn 化"
        );
    }

    #[test]
    fn the_returned_values_are_contract_views_not_serde_leftovers() {
        let code = trait_code();
        // 契约视图来自 platform-api 的 dto 词汇表，应用层不再另造一套形状。
        for view in [
            "Result<Vec<ProfileView>, ApiError>",
            "Result<SessionView, ApiError>",
            "Result<ExecutionView, ApiError>",
            "Result<ExecutionReceipt, ApiError>",
            "Result<OpenSessionReceipt, ApiError>",
            "Result<ContextChangeReceipt, ApiError>",
            "Result<CloseReceipt, ApiError>",
            "Result<CancelReceipt, ApiError>",
            "Result<SubmissionToken, ApiError>",
            "Result<EventSubscription, ApiError>",
        ] {
            assert!(code.contains(view), "返回类型必须是契约视图：{view}");
        }
    }
}
