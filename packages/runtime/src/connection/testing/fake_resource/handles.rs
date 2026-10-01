//! 句柄铸造与「登记 vs 造句柄」（fake-runtime-fixtures.md §5.1 L416、§9.1、§9.2）。
//!
//! §5.1 要求假提供方**诚实报告句柄**：一个执行如果真的造出了 session handle，
//! 就必须在 `ExecutionCompletion.sessionHandles` 里如实上报。因此这里把两件事拆开：
//!
//! - **造句柄**（[`FakeResourceProvider::mint_handle`]）：由提供方铸造 `HandleId`，
//!   返回一个尚未进入登记册的 `SessionHandleRef`，语义是「这个执行确实开了一个句柄」。
//! - **登记句柄**（[`FakeResourceProvider::register_handle`]）：把句柄放进
//!   `FakeResource.handles`，并在 journal 写 `registered`。
//!   [`FakeResourceProvider::orphan_handle`] 只造句柄、写 `orphaned`，**不**登记 ——
//!   这正是 §9.2 反例命令 `begin_session_transaction_unregistered` 需要的形状。
//!
//! 另有一类操作是「用别的资源/别的 epoch 的句柄」（§9.2 的
//! `commit_with_stale_handle` 与 `handle_from_other_resource`）：它们由
//! [`FakeResourceProvider::resolve`] 判定，按 §3.1「`ResourceHandle` 只由提供方签发，
//! 每次操作都要校验 `resourceId` + `runtimeEpoch` + owner」执行。

use crate::connection::error::ProviderError;
use crate::connection::port::{PermitSet, ResourceHandle, Secret};
use crate::connection::session::{HandleKind, SessionHandleRef, SessionView};
use crate::connection::types::{ExecutionId, HandleId, ResourceId};

use super::state::{FakeResource, FakeResourceState};
// `FakeResourceProvider` 定义在父模块（`mod.rs`）。子模块的 `impl` 块必须显式导入
// 自类型：否则整个 impl 解析不到，块内 `register_handle` / `verify_handle` / `resolve`
// 会同时退化成下游调用点上的「no method named ... on `&FakeResourceProvider`」。
use super::FakeResourceProvider;

/// `acquireResource` 的返回值。
///
/// §8.2 L441：`attachmentToken` 是全系统唯一真正随机的值，所以它包在
/// [`Secret`] 里 —— `Secret` 只有 `expose()` 一个出口，没有 `Clone`/`Display`
/// 明文实现，journal 与任何 `Debug` 输出都拿不到它的内容（§13）。
#[derive(Debug)]
pub struct AcquiredResource {
    pub resource_id: ResourceId,
    pub handle: ResourceHandle,
    pub permits: PermitSet,
    pub attachment_token: Secret,
    pub session: SessionView,
}

impl FakeResourceProvider {
    /// 造一个句柄并按 §3.1 由提供方签发对应的 `ResourceHandle`（只读校验凭证）。
    /// 句柄本身**尚未**登记 —— 调用方决定是 `register_handle` 还是 `orphan_handle`。
    pub fn mint_handle(
        &self,
        resource_id: &ResourceId,
        kind: HandleKind,
        handle_id: crate::connection::types::HandleId,
    ) -> Result<SessionHandleRef, ProviderError> {
        let resources = self.lock();
        let resource = resources.get(resource_id.as_str()).ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {} 不存在，无法铸造句柄",
                resource_id.as_str()
            ))
        })?;
        Ok(SessionHandleRef::new(
            handle_id,
            kind,
            resource.resource_id.clone(),
            resource.runtime_epoch,
        ))
    }

    /// 造句柄 **并** 登记：进 `FakeResource.handles`，journal 写 `registered`（§5.3 规则 5）。
    pub fn register_handle(
        &self,
        resource_id: &ResourceId,
        kind: HandleKind,
        handle_id: crate::connection::types::HandleId,
        execution_id: Option<&ExecutionId>,
    ) -> Result<SessionHandleRef, ProviderError> {
        let mut resources = self.lock();
        let resource = resources.get_mut(resource_id.as_str()).ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {} 不存在，无法登记句柄",
                resource_id.as_str()
            ))
        })?;
        let handle = SessionHandleRef::new(
            handle_id.clone(),
            kind,
            resource.resource_id.clone(),
            resource.runtime_epoch,
        );
        resource.register_handle(handle.clone());
        // journal 有自己的锁，不与资源表锁构成重入。
        self.journal.record_handle_for(
            &handle,
            super::super::journal::HandleAction::Registered,
            "注册会话句柄",
            execution_id,
        );
        Ok(handle)
    }

    /// §9.2 反例：造句柄但**不**登记，journal 写 `orphaned`。
    /// 关闭路径随后必须把它收回，否则 §4.3 的 I7 不成立。
    pub fn orphan_handle(
        &self,
        resource_id: &ResourceId,
        kind: HandleKind,
        handle_id: crate::connection::types::HandleId,
    ) -> Result<SessionHandleRef, ProviderError> {
        let resources = self.lock();
        let resource = resources.get(resource_id.as_str()).ok_or_else(|| {
            ProviderError::SessionLost(format!("资源 {} 不存在，无法造句柄", resource_id.as_str()))
        })?;
        let handle = SessionHandleRef::new(
            handle_id,
            kind,
            resource.resource_id.clone(),
            resource.runtime_epoch,
        );
        self.journal.record_handle(
            &handle,
            super::super::journal::HandleAction::Orphaned,
            "反例：造句柄但不登记",
        );
        Ok(handle)
    }

    /// 注销句柄：幂等。§5.3 规则 6 要求写 `closed` 事件并从登记册移除。
    pub fn close_handle(
        &self,
        resource_id: &ResourceId,
        handle_id: &crate::connection::types::HandleId,
        reason: &str,
    ) -> Result<SessionHandleRef, ProviderError> {
        let mut resources = self.lock();
        let resource = resources.get_mut(resource_id.as_str()).ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {} 不存在，无法关闭句柄",
                resource_id.as_str()
            ))
        })?;
        let key = handle_id.as_str().to_owned();
        let Some(mut handle) = resource.deregister_handle(&key) else {
            return Err(ProviderError::SessionNotFound(format!(
                "句柄 {} 未登记在资源 {} 上",
                handle_id.as_str(),
                resource_id.as_str()
            )));
        };
        handle.closed = true;
        if handle.kind == HandleKind::Transaction {
            resource.transaction_state = crate::connection::session::TransactionState::None;
        }
        self.journal
            .record_handle(&handle, super::super::journal::HandleAction::Closed, reason);
        Ok(handle)
    }

    pub fn registered_handles(&self, resource_id: &ResourceId) -> usize {
        self.lock()
            .get(resource_id.as_str())
            .map(|resource| resource.registered_handles())
            .unwrap_or(0)
    }

    /// 该资源上还开着（未注销）的句柄 id，按 `HandleId` 升序 —— 注销顺序因此是确定的，
    /// 不依赖 `HashMap` 迭代序，journal 里的 `handle closed` 顺序可复现。
    ///
    /// §9.3 / CM-73 的关闭路径用它来「先注销句柄、再释放资源」。
    pub fn open_handle_ids(&self, resource_id: &ResourceId) -> Vec<HandleId> {
        let resources = self.lock();
        let Some(resource) = resources.get(resource_id.as_str()) else {
            return Vec::new();
        };
        let mut ids: Vec<HandleId> = resource
            .open_handles()
            .into_iter()
            .map(|handle| handle.handle_id.clone())
            .collect();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids
    }

    /// §3.1 的「每次操作都要校验」对外暴露的只读入口。
    ///
    /// 命令网关在改动任何状态之前先跑这一道：凭证失效的资源不应该被写脏。
    /// 校验口径与 [`FakeResourceProvider::resolve`] 完全一致
    /// （`resourceId` + `runtimeEpoch` + owner，且资源必须还活着）。
    pub fn verify_handle(&self, handle: &ResourceHandle) -> Result<(), ProviderError> {
        self.resolve(&handle.resource_id, handle, false).map(|_| ())
    }

    /// §3.1：每次操作都要用 `ResourceHandle` 校验 `resourceId` + `runtimeEpoch` + owner。
    ///
    /// - `claimed` 是**调用方声称**的资源 id，`handle` 是它出示的凭证。
    /// - 两者不一致（§9.2 `handle_from_other_resource`）→ `SessionLost`。
    /// - epoch 不一致（§9.2 `commit_with_stale_handle`）→ `RuntimeEpochMismatch`。
    /// - 资源已 `Closed` 且调用方没有声明 `allow_closed`（幂等关闭路径）→ `SessionLost`。
    pub(crate) fn resolve(
        &self,
        claimed: &ResourceId,
        handle: &ResourceHandle,
        allow_closed: bool,
    ) -> Result<FakeResource, ProviderError> {
        let resource = self.lock().get(claimed.as_str()).cloned().ok_or_else(|| {
            ProviderError::SessionLost(format!("资源 {} 不存在，句柄无法验证", claimed.as_str()))
        })?;
        let owner = resource.owner_ref().cloned().ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {} 没有 owner，无法验证句柄",
                claimed.as_str()
            ))
        })?;
        // `verify` 在 resourceId 不符时先报 SessionLost、epoch 不符时再报 RuntimeEpochMismatch，
        // 正好是 §9.2 两条反例各自期望的错误码。
        handle.verify(claimed, &resource.runtime_epoch, &owner)?;
        if resource.state == FakeResourceState::Closed && !allow_closed {
            return Err(ProviderError::SessionLost(format!(
                "资源 {} 已关闭，不再接受操作",
                claimed.as_str()
            )));
        }
        Ok(resource)
    }
}
