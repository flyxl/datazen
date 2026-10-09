//! FakeHarness —— 假运行时夹具的顶层入口。
//!
//! # 为什么这里没有实现 `DatabaseDriver`
//!
//! 会话级句柄命令要求「通过 driver-api 的 `command_definitions()` / `execute_command()`
//! 通道暴露」。P0 阶段本仓库里**没有任何**以 `datazen-runtime` 夹具为后端的 driver crate，
//! 要走那条通道就必须先造一个假 driver crate —— 这既超出 P0 的铁律（只加夹具与测试、
//! 不改生产执行路径、不得让 `datazen` 或任何 driver crate 依赖新 crate），
//! 也会给模块表凭空加一个节点。
//!
//! 因此这里实现的是**同一条语义路径的夹具侧落点**：[`FakeHarness::invoke`] 依次做
//!
//! 1. 用 `commands::session_handle_command_definition(id)` 查表拿 `DriverCommandDefinition`；
//! 2. 调 driver-api 的 `validate_command_input(&definition, &input)` 做 schema 校验
//!    （缺必填字段、非对象入参都会在这里被拒，错误串与真 driver 一致）；
//! 3. 按命令表里的语义分发到 `FakeResourceProvider` 的九个操作之一；
//! 4. 用 `CommandResult::new(data)` 交出结果。
//!
//! 也就是说，**输入校验与命令定义完全复用 driver-api，没有另写一套**，
//! 只是省掉了「为了测试而注册一个 inventory driver」这一层。将来真 driver 落地时，
//! 把 `invoke` 换成 `execute_command()` 调用即可，其余（表、schema、json 形状）不用动。
//!
//! # 为什么 `assert_no_leak` 必须显式调用
//!
//! 泄漏不变量检查要求在每个故障用例末尾**显式**调用 `assert_no_leak()`，而不是靠 `Drop`。
//! `Drop` 在 panic 展开时会把顺序反过来（资源先于断言释放），断言就失去了意义。
//! 所以这里**不实现** `Drop`，也刻意不提供 `#[must_use]` 之类的软提示：
//! 用例不调用就没有断言，这正是要的形状。

mod cm73;
#[cfg(test)]
mod cm73_threads;
#[cfg(test)]
mod cm74_release_order;
mod session_cmds;
#[cfg(test)]
mod tests;

use datazen_driver_api::command::{validate_command_input, CommandResult};
use serde_json::{json, Value as JsonValue};

use crate::connection::error::ProviderError;
use crate::connection::execution::SessionCommand;
use crate::connection::port::{
    AcquireResourceRequest, BudgetClass, CloseReceipt, CloseResourceRequest,
};
use crate::connection::testing::barrier::Barrier;
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::commands::session_handle_command_definition;
use crate::connection::testing::fake_resource::{
    AcquiredResource, FakeResourceProvider, FakeScript,
};
#[cfg(test)]
use crate::connection::testing::fixtures;
use crate::connection::testing::ids::FakeIds;
use crate::connection::testing::journal::CommandJournal;
use crate::connection::types::{
    ExecutionTarget, HandleId, OwnerRef, PoolKeyFingerprint, PoolKeyInputs, WorkerId,
};
#[cfg(test)]
use crate::connection::types::{JobId, OrganizationId};

pub use cm73::{EvictionRaceOutcome, EvictionRaceReport};
pub use session_cmds::GatewayError;

use session_cmds::{handle_id_field, str_field, u64_field};

/// 夹具顶层句柄。
///
/// 拥有 provider **本体**（它不是 `Clone`：内部有 `AtomicU64` 与多个 `Mutex`），
/// 再加一个 [`Barrier`] 供关闭 / 驱逐的竞态编排。
/// `clock` / `ids` / `journal` / `script` 全部从 provider 取，因此
/// 「推进假时钟」与「台账里的单调时间」天然同源。
pub struct FakeHarness {
    provider: FakeResourceProvider,
    barrier: Barrier,
}

impl FakeHarness {
    /// 构造一个夹具。`target` 是所有假资源要绑定的执行目标
    /// （`AcquireResourceRequest` 没有 target 字段，目标在提供方构造时固定）。
    pub fn new(worker_id: &str, target: ExecutionTarget) -> Self {
        Self {
            provider: FakeResourceProvider::new(WorkerId::new(worker_id), target),
            barrier: Barrier::new(),
        }
    }

    /// 用 builder 把 provider 装好再接管（执行身份、共享台账、configRevision）。
    pub fn from_provider(provider: FakeResourceProvider) -> Self {
        Self {
            provider,
            barrier: Barrier::new(),
        }
    }

    // ---- 只读访问器 ----

    pub fn provider(&self) -> &FakeResourceProvider {
        &self.provider
    }

    pub fn journal(&self) -> &CommandJournal {
        self.provider.journal()
    }

    pub fn clock(&self) -> &FakeClock {
        self.provider.clock()
    }

    pub fn ids(&self) -> &FakeIds {
        self.provider.ids()
    }

    pub fn script(&self) -> &FakeScript {
        self.provider.script()
    }

    pub fn barrier(&self) -> &Barrier {
        &self.barrier
    }

    // ---- 常用装配 ----

    /// `PoolKeyInputs` 派生。`policy_isolation_key` 单列，
    /// 因为要求执行身份相同、策略隔离不同的两个用户能落到不同的池键。
    pub fn pool_key(&self, policy_isolation_key: &str) -> PoolKeyFingerprint {
        let target = self.provider.target();
        PoolKeyFingerprint::derive(&PoolKeyInputs {
            connection_id: target.connection_id.clone(),
            config_revision: self.provider.config_revision(),
            driver_id: crate::connection::testing::fake_resource::PROVIDER_ID.to_owned(),
            namespace: target.namespace.clone(),
            execution_identity_key: self.provider.execution_identity().to_owned(),
            policy_isolation_key: policy_isolation_key.to_owned(),
        })
    }

    /// 申请一个资源。`db_session_id` 由 `FakeIds` 发放（形如 `dbs_<workerId>_<seq:04>`）。
    pub fn acquire(
        &self,
        owner: OwnerRef,
        policy_isolation_key: &str,
        budget_class: BudgetClass,
    ) -> Result<AcquiredResource, ProviderError> {
        let request = AcquireResourceRequest {
            descriptor: self.provider.descriptor(),
            pool_key: self.pool_key(policy_isolation_key),
            budget_class,
            owner,
            db_session_id: self.ids().next_db_session_id(),
        };
        self.provider.acquire(&request)
    }

    /// 关闭资源。**先注销该资源上还开着的句柄，再释放资源** ——
    /// 「淘汰、替换、隔离时必须先在原 resource 上回滚/关闭句柄并注销，
    /// 确认后才释放资源」；对 `open_session_cursor` 的断言也写明
    /// 「关闭游标后 `handles` 为空才允许 `Clean` 归池」。
    ///
    /// 句柄注销走 [`FakeResourceProvider::close_handle`]，因此 journal 里
    /// `handle closed` 排在 `resource Closed` **之前**，`registered_handles`
    /// 也是注销之后重新取的，所以 `ReturnedToPool` 的前置条件是真实成立的，
    /// 不是靠调用方填一个 0 骗过去的。
    ///
    /// 返回 `ProviderError` 而不是 panic：调用方（用例）自己决定怎么断言。
    pub fn close(
        &self,
        handle: &crate::connection::port::ResourceHandle,
    ) -> Result<CloseReceipt, ProviderError> {
        let resource_id = handle.resource_id.clone();
        let open: Vec<HandleId> = self.provider.open_handle_ids(&resource_id);
        for handle_id in open {
            self.provider
                .close_handle(&resource_id, &handle_id, "关闭资源前先注销句柄")?;
        }
        self.provider.close_resource(&CloseResourceRequest {
            handle: handle.clone(),
            registered_handles: self.provider.registered_handles(&resource_id),
            protocol_drained: true,
        })
    }

    // ---- 命令网关 ----

    /// 走会话级句柄命令：查表 → 校验入参 → 分发 → `CommandResult`。
    ///
    /// `resource_handle` 必须是**由该提供方签发**的只读凭证。
    /// `input` 就是表里那条命令的入参 schema —— `handleId`、`rows`、`name`、
    /// `holdMs`、`runtimeEpoch`、`resourceId` 全部从 `input` 里读，和真实
    /// `execute_command` 的调用方式一致；`validate_command_input` 已经保证
    /// schema 标了 `required` 的字段一定在。
    pub fn invoke(
        &self,
        command: SessionCommand,
        resource_handle: &crate::connection::port::ResourceHandle,
        input: JsonValue,
    ) -> Result<CommandResult, GatewayError> {
        let definition = session_handle_command_definition(command.id())
            .ok_or_else(|| GatewayError::UnknownCommand(command.id().to_owned()))?;
        validate_command_input(&definition, &input).map_err(GatewayError::InvalidInput)?;

        let data = match command {
            SessionCommand::BeginSessionTransaction => self.begin_transaction(resource_handle)?,
            SessionCommand::BeginSessionTransactionHold => {
                let hold_ms = u64_field(&input, "holdMs")?;
                // hold 命令**不自动终结**事务，它的存在就是为了和关闭 / 驱逐赛跑。
                // 只推进假单调时钟，绝不 sleep。
                self.clock()
                    .advance(std::time::Duration::from_millis(hold_ms));
                self.begin_transaction(resource_handle)?
            }
            SessionCommand::OpenSessionCursor => {
                let rows = u64_field(&input, "rows")?;
                self.open_cursor(resource_handle, rows)?
            }
            SessionCommand::PrepareServerStatement => {
                let name = str_field(&input, "name")?;
                self.prepare_server_statement(resource_handle, name)?
            }
            SessionCommand::CommitSessionTransaction => {
                self.commit_transaction(resource_handle, &handle_id_field(&input)?)?
            }
            SessionCommand::RollbackSessionTransaction => {
                self.rollback_transaction(resource_handle, &handle_id_field(&input)?)?
            }
            SessionCommand::CloseSessionCursor => {
                self.close_cursor(resource_handle, &handle_id_field(&input)?)?
            }
            // 下面三条是**反例命令**，它们存在的目的就是被判负。
            SessionCommand::BeginSessionTransactionUnregistered => {
                self.begin_transaction_unregistered(resource_handle)?
            }
            SessionCommand::CommitWithStaleHandle => {
                let stale_epoch =
                    crate::connection::types::Counter::new(u64_field(&input, "runtimeEpoch")?);
                self.commit_with_stale_epoch(
                    resource_handle,
                    &handle_id_field(&input)?,
                    stale_epoch,
                )?
            }
            SessionCommand::HandleFromOtherResource => {
                let other = str_field(&input, "resourceId")?;
                self.handle_from_other_resource(resource_handle, other, &handle_id_field(&input)?)?
            }
        };
        Ok(CommandResult::new(data))
    }

    // ---- 泄漏不变量 ----

    /// 泄漏不变量检查。**必须由用例显式调用**，不用 `Drop`。
    ///
    /// 返回 `Err(消息)` 而不是直接 panic，是为了让**要对比多个检查点**的用例
    /// 能把「停住时的状态」和「收口后的状态」都拿到手再断言。
    pub fn assert_no_leak(&self) -> Result<(), String> {
        // 全部泄漏不变式一次算清：`leak_invariant_violations` 内部依次检查
        // live 资源 / live lease / 活动会话 / 登记句柄 / permit 收支 / 孤立句柄 / 事件序号连续。
        let mut violations: Vec<String> =
            self.provider.journal().assert().leak_invariant_violations();
        // 变化点断言：预算记账与句柄登记的每一步都必须留证据。
        violations.extend(self.provider.journal().assert().change_point_violations());

        if violations.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "泄漏不变量不成立（逐条列出）：\n  - {}",
                violations.join("\n  - ")
            ))
        }
    }
}

/// 便捷构造：`NS_A` 命名空间 + 指定 connection 的执行目标。
///
/// 目标值一律取自 [`crate::connection::testing::fixtures`]，用例里不许再出现硬编码字面量。
///
/// 只在 `cfg(test)` 下编译：本函数目前只有同 crate 的用例调用，挂在
/// `feature = "test-harness"` 上会让「不带 --features 的 `cargo check`」报死代码。
#[cfg(test)]
pub fn fixture_target(namespace_key: &str) -> ExecutionTarget {
    let catalog = crate::connection::testing::fixtures::fixtures();
    let namespace = catalog
        .namespaces
        .iter()
        .find(|ns| ns.key == namespace_key)
        .unwrap_or_else(|| panic!("fixtures 里没有命名空间 {namespace_key}"));
    let profile = catalog
        .profiles
        .first()
        .unwrap_or_else(|| panic!("fixtures 里没有连接配置"));
    ExecutionTarget {
        connection_id: profile.connection_id.clone(),
        namespace: crate::connection::types::NamespaceTarget {
            database: namespace.database.to_owned(),
            catalog: String::new(),
            schema: String::new(),
            path: String::new(),
        },
        object: None,
    }
}

// ---------------------------------------------------------------------------
// 跨用例模块共享的私有帮助函数
// ---------------------------------------------------------------------------
//
// 这三个帮助函数被 `tests.rs` 与 `cm74_release_order.rs` **同时**调用。
// 放这里而不是留在其中一边，是因为留在一边就得让另一边 `use super::tests::…`，
// 或者把同一个函数抄两份 —— 两者都把「同一个夹具世界」的事实写成了巧合。
// 这与 [`fixture_target`] 是同一条理由：只被用例调用、但用例分居多个模块的帮助函数归本文件。

/// 夹具目标取自 `fixtures`，用例里不写硬编码命名空间字面量。
#[cfg(test)]
pub fn harness_for(namespace_key: &str) -> FakeHarness {
    FakeHarness::from_provider(
        FakeResourceProvider::new(WorkerId::new("w1"), fixture_target(namespace_key))
            .with_execution_identity(fixtures::IDENTITY_SHARED),
    )
}

/// `PROFILE_P` 归属：一个 job owner。
#[cfg(test)]
pub fn owner() -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(fixtures::ORG_A),
        job_id: JobId::new("job_org-alpha_0001"),
        stage_id: "job:job_org-alpha_0001/stage:1".to_owned(),
    }
}

/// 从 `CommandResult.data` 里取第一条会话句柄的 id。
///
/// output schema 已经声明了 `sessionHandles[].handleId`，所以这里
/// 只做形状检查；形状漂移用 `expect` 报出来是刻意的 —— 它意味着命令表变了。
#[cfg(test)]
pub fn first_handle_id(result: &CommandResult) -> String {
    result
        .data
        .pointer("/sessionHandles/0/handleId")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| {
            panic!(
                "命令输出里没有 sessionHandles[0].handleId，实际是 {}",
                result.data
            )
        })
        .to_owned()
}

/// 供 `session_cmds` 组装输出用的最小 json 帮助函数。
pub(crate) fn handle_payload(handle: &crate::connection::session::SessionHandleRef) -> JsonValue {
    json!({
        "handleId": handle.handle_id.as_str(),
        "kind": handle.kind.as_str(),
        "resourceId": handle.resource_id.as_str(),
        "runtimeEpoch": handle.runtime_epoch.get(),
    })
}
