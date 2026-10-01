//! §9.3 CM-73「空闲驱逐 vs 会话事务」竞态的编排与断言。
//!
//! §9.3 规定的五步是：
//!
//! 1. 会话 + pinned Lease 落在 **R1** 上（记录 `runtimeEpoch` 的实际取值，不硬编码 1 ——
//!    `FakeIds::force_collision` 可以把它推高，硬编码会把夹具写死）；
//! 2. 空闲扫描把 R1 推到**关闭前检查点**，barrier 停住；
//! 3. 停住那一刻发起 `begin_session_transaction`，事务**必须落在 R1 上**；
//! 4. 放开驱逐：先在 R1 上回滚 + 注销句柄，**然后**才归还 R1；
//! 5. 宿主重建 **R2** 走恢复路径 —— 台账上 R2 不得出现任何 `registered` 句柄事件，
//!    恢复出来的 `dbSessionId` 必须是**新的**。
//!
//! 整段编排是**行为驱动**的：没有 sleep、没有 `ConnectionManager`、没有真实线程，
//! 顺序完全由 journal 的 `seq` 表达。将来把 `ConnectionManager` 删掉，这些断言
//! 一条都不需要改 —— 它们看的是台账，不是实现。
//!
//! 反例（宿主把旧句柄搬到 R2 上复用）由 [`FakeHarness::assert_no_handle_reuse`]
//! 负责判负，负向用例在 `tests.rs` 里显式造出来，不靠「相信正例不会出错」。

use serde_json::{json, Value as JsonValue};

use crate::connection::error::ProviderError;
use crate::connection::execution::SessionCommand;
use crate::connection::port::{BudgetClass, CloseReceipt, CloseResourceRequest, ResourceHandle};
use crate::connection::testing::journal::{HandleAction, JournalEntry};
use crate::connection::types::{Counter, DbSessionId, HandleId, LeaseId, OwnerRef, ResourceId};

use super::{FakeHarness, GatewayError};

/// 一次 CM-73 竞态跑完之后的判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionRaceOutcome {
    /// 正例：R2 上没有任何句柄登记，旧句柄没有被搬过去。
    HandlesNotReused,
    /// 反例：R2 上出现了句柄登记，§9.3 的断言必须判负。
    HandlesReused { registered_on_recovery: usize },
}

/// 一次 CM-73 竞态的全部可核对事实。
///
/// 断言全部针对这些**已记账的量**，不针对内存布局：换实现、换线程模型都不影响。
#[derive(Debug, Clone)]
pub struct EvictionRaceReport {
    pub pre_close_resource_id: ResourceId,
    pub pre_close_epoch: Counter,
    pub pre_close_db_session_id: DbSessionId,
    pub pre_close_lease: LeaseId,
    /// 停住那一刻新开的事务句柄（§9.3 第 3 步）。
    pub race_handle_id: HandleId,
    pub recovery_resource_id: ResourceId,
    /// R2 的只读凭证。收尾时用它把恢复资源也走一遍正常关闭路径 ——
    /// 否则 I2（live 资源）、I4（active session）、I1/I6（permit 收支）永远收不了口。
    pub recovery_handle: ResourceHandle,
    pub recovery_epoch: Counter,
    pub recovery_db_session_id: DbSessionId,
    pub registered_on_recovery: usize,
    pub outcome: EvictionRaceOutcome,
}

/// 从 `CommandResult.data` 里取第一条句柄的 id。
///
/// `validate_command_input` 只保证**输入**合法，输出形状由 `commands.rs` 的 output schema
/// 约定；这里显式报错而不是 `unwrap()`，形状漂移要当场炸出来。
fn first_handle_id(data: &JsonValue) -> Result<HandleId, GatewayError> {
    data["sessionHandles"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["handleId"].as_str())
        .map(HandleId::new)
        .ok_or_else(|| {
            GatewayError::InvalidInput(
                "begin_session_transaction 没有交出 sessionHandles".to_owned(),
            )
        })
}

impl FakeHarness {
    /// §9.3 的五步竞态，返回可核对的报告。
    ///
    /// `hold_ms` 交给 `begin_session_transaction_hold`：那条命令**不会**自动终结事务，
    /// 它的存在就是为了和关闭 / 驱逐赛跑。假时钟只前进不 sleep。
    pub fn script_hold_for_eviction_then_begin_commit(
        &self,
        owner: OwnerRef,
        policy_isolation_key: &str,
        hold_ms: u64,
    ) -> Result<EvictionRaceReport, GatewayError> {
        let race = self.script().hold_for_eviction_then_begin_commit();
        assert!(
            race.rollback_before_release,
            "CM-73 脚本必须要求「先回滚注销、再归还资源」，否则第 4 步的顺序断言失去意义"
        );

        // ---- 第 1 步：会话 + pinned lease 落在 R1 ----
        let acquired = self
            .acquire(owner.clone(), policy_isolation_key, BudgetClass::Session)
            .map_err(GatewayError::Provider)?;
        let pre_close_lease = self.provider().pin_lease(&acquired.resource_id);
        let pre_close_epoch = acquired.handle.runtime_epoch;
        let pre_close_db_session_id = acquired.session.handle.db_session_id.clone();

        // ---- 第 2 步：空闲扫描推到关闭前检查点，停住 ----
        self.barrier().arrive(&race.pre_close);

        // ---- 第 3 步：停住那一刻开会话事务（必须落在 R1）----
        let opened = self.invoke(
            SessionCommand::BeginSessionTransactionHold,
            &acquired.handle,
            json!({ "holdMs": hold_ms }),
        )?;
        let race_handle_id = first_handle_id(&opened.data)?;

        // ---- 第 4 步：放开驱逐，先回滚注销、再归还 R1 ----
        self.barrier().arrive(&race.release_close);
        self.barrier().release(&race.release_close);
        let close = self.evict_idle_resource(&acquired.handle)?;
        if close.resource_release != crate::connection::port::ResourceRelease::Confirmed {
            return Err(GatewayError::Provider(ProviderError::CleanupFailed(
                format!(
                    "R1 关闭未确认（{:?}），无法进入恢复路径",
                    close.resource_release
                ),
            )));
        }

        // ---- 第 5 步：恢复路径重建 R2 ----
        let recovered = self
            .acquire(owner, policy_isolation_key, BudgetClass::Session)
            .map_err(GatewayError::Provider)?;
        let reused = self.registered_handle_ids_on(&recovered.resource_id);

        Ok(EvictionRaceReport {
            pre_close_resource_id: acquired.resource_id,
            pre_close_epoch,
            pre_close_db_session_id,
            pre_close_lease,
            race_handle_id,
            registered_on_recovery: reused.len(),
            outcome: if reused.is_empty() {
                EvictionRaceOutcome::HandlesNotReused
            } else {
                EvictionRaceOutcome::HandlesReused {
                    registered_on_recovery: reused.len(),
                }
            },
            recovery_resource_id: recovered.resource_id,
            recovery_epoch: recovered.handle.runtime_epoch,
            recovery_handle: recovered.handle,
            recovery_db_session_id: recovered.session.handle.db_session_id.clone(),
        })
    }

    /// 空闲驱逐的标准收尾：从资源当前快照取归池前置，再走 `closeResource`。
    ///
    /// 之所以不在这里硬编码 `registered_handles = 0`：`closeResource` 内部会在
    /// 回滚注销**之后**重算一次，传进去的值只是调用方的声明。
    pub fn evict_idle_resource(
        &self,
        handle: &ResourceHandle,
    ) -> Result<CloseReceipt, ProviderError> {
        let snapshot = self
            .provider()
            .resource(&handle.resource_id)
            .ok_or_else(|| {
                ProviderError::SessionLost(format!(
                    "资源 {} 不存在，无法驱逐",
                    handle.resource_id.as_str()
                ))
            })?;
        self.provider().close_resource(&CloseResourceRequest {
            handle: handle.clone(),
            registered_handles: snapshot.registered_handles(),
            protocol_drained: snapshot.protocol_drained,
        })
    }

    /// 某个资源上被 `Registered` 过的句柄 id（按台账 `seq` 顺序）。
    pub fn registered_handle_ids_on(&self, resource_id: &ResourceId) -> Vec<HandleId> {
        self.journal()
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                JournalEntry::Handle {
                    resource_id: owner,
                    handle_id,
                    action: HandleAction::Registered,
                    ..
                } if owner == resource_id => Some(HandleId::new(handle_id.clone())),
                _ => None,
            })
            .collect()
    }

    /// §9.3 的核心断言：恢复出来的资源上**不得**出现任何句柄登记。
    ///
    /// 报不出 `Ok` 就是判负 —— 用例只调用这一个方法，不另写一份判负逻辑。
    pub fn assert_no_handle_reuse(&self, recovery_resource_id: &ResourceId) -> Result<(), String> {
        let reused = self.registered_handle_ids_on(recovery_resource_id);
        if reused.is_empty() {
            return Ok(());
        }
        let listed = reused
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "§9.3 判负：恢复资源 {} 上出现 {} 条句柄登记（{listed}）—— 驱逐后不得复用旧句柄",
            recovery_resource_id.as_str(),
            reused.len()
        ))
    }
}
