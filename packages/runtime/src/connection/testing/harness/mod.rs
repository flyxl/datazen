//! FakeHarness —— 假运行时夹具的顶层入口（fake-runtime-fixtures.md §2 的 `H` 节点、§4.3、§9）。
//!
//! # 为什么这里没有实现 `DatabaseDriver`
//!
//! §9.1 要求会话级句柄命令「通过 driver-api 的 `command_definitions()` / `execute_command()`
//! 通道暴露」。P0 阶段本仓库里**没有任何**以 `datazen-runtime` 夹具为后端的 driver crate，
//! 要走那条通道就必须先造一个假 driver crate —— 这既超出 P0 的铁律（只加夹具与测试、
//! 不改生产执行路径、不得让 `datazen` 或任何 driver crate 依赖新 crate），
//! 也会给 §2 的模块表凭空加一个节点。
//!
//! 因此这里实现的是**同一条语义路径的夹具侧落点**：[`FakeHarness::invoke`] 依次做
//!
//! 1. 用 `commands::session_handle_command_definition(id)` 查表拿 `DriverCommandDefinition`；
//! 2. 调 driver-api 的 `validate_command_input(&definition, &input)` 做 schema 校验
//!    （缺必填字段、非对象入参都会在这里被拒，错误串与真 driver 一致）；
//! 3. 按 §9.1 的语义分发到 `FakeResourceProvider` 的九个操作之一；
//! 4. 用 `CommandResult::new(data)` 交出结果。
//!
//! 也就是说，**输入校验与命令定义完全复用 driver-api，没有另写一套**，
//! 只是省掉了「为了测试而注册一个 inventory driver」这一层。将来真 driver 落地时，
//! 把 `invoke` 换成 `execute_command()` 调用即可，其余（表、schema、json 形状）不用动。
//!
//! # 为什么 `assert_no_leak` 必须显式调用
//!
//! §4.3 明确要求在每个故障用例末尾**显式**调用 `assert_no_leak()`，而不是靠 `Drop`。
//! `Drop` 在 panic 展开时会把顺序反过来（资源先于断言释放），断言就失去了意义。
//! 所以这里**不实现** `Drop`，也刻意不提供 `#[must_use]` 之类的软提示：
//! 用例不调用就没有断言，这是 §4.3 要的形状。

mod cm73;
mod session_cmds;
#[cfg(test)]
mod tests;

use datazen_driver_api::command::{validate_command_input, CommandResult};
use serde_json::{json, Value as JsonValue};

use crate::connection::error::ProviderError;
use crate::connection::execution::SessionCommand;
use crate::connection::port::{AcquireResourceRequest, BudgetClass};
use crate::connection::testing::barrier::Barrier;
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::commands::session_handle_command_definition;
use crate::connection::testing::fake_resource::{
    AcquiredResource, FakeResourceProvider, FakeScript,
};
use crate::connection::testing::ids::FakeIds;
use crate::connection::testing::journal::CommandJournal;
use crate::connection::types::{
    ExecutionTarget, OwnerRef, PoolKeyFingerprint, PoolKeyInputs, WorkerId,
};

pub use cm73::{EvictionRaceReport, EvictionRaceOutcome};
pub use session_cmds::GatewayError;

/// 夹具顶层句柄。
///
/// 拥有 provider **本体**（它不是 `Clone`：内部有 `AtomicU64` 与多个 `Mutex`），
/// 再加一个 [`Barrier`] 供 CM-73 / CM-74 的竞态编排。
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
        Self { provider, barrier: Barrier::new() }
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

    /// §8.1 的 `PoolKeyInputs` 派生。`policy_isolation_key` 单列，
    /// 因为 CM-05 / CM-67 要求执行身份相同、策略隔离不同的两个用户能落到不同的池键。
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

    /// 申请一个资源。`db_session_id` 由 `FakeIds` 发放（§8.2 `dbs_<workerId>_<seq:04>`）。
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

    // ---- §9.1 命令网关 ----

    /// 走 §9.1 的会话级句柄命令：查表 → 校验入参 → 分发 → `CommandResult`。
    ///
    /// `resource_handle` 必须是**由该提供方签发**的只读凭证（§3.1）；
    /// `handle_id` 是 §9.1 表里那条命令要操作的句柄（事务 / 游标句柄）。
    pub fn invoke(
        &self,
        command: SessionCommand,
        resource_handle: &crate::connection::port::ResourceHandle,
        handle_id: &crate::connection::types::HandleId,
        input: JsonValue,
    ) -> Result<CommandResult, GatewayError> {
        let definition = session_handle_command_definition(command.id())
            .ok_or_else(|| GatewayError::UnknownCommand(command.id().to_owned()))?;
        validate_command_input(&definition, &input).map_err(GatewayError::InvalidInput)?;

        let data = match command {
            SessionCommand::BeginSessionTransaction => {
                self.begin_transaction(resource_handle)?
            }
            SessionCommand::BeginSessionTransactionHold => {
                let hold_ms = input
                    .get("holdMs")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| GatewayError::InvalidInput("holdMs 必须是整数".to_owned()))?;
                // §9.1：hold 命令**不自动终结**事务，它的存在就是为了和关闭 / 驱逐赛跑。
                // 只推进假单调时钟，绝不 sleep。
                self.clock().advance(std::time::Duration::from_millis(hold_ms));
                self.begin_transaction(resource_handle)?
            }
            SessionCommand::OpenSessionCursor => {
                let rows = input
                    .get("rows")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| GatewayError::InvalidInput("rows 必须是整数".to_owned()))?;
                self.open_cursor(resource_handle, rows)?
            }
            SessionCommand::PrepareServerStatement => {
                let name = input
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| GatewayError::InvalidInput("name 必须是字符串".to_owned()))?;
                self.prepare_server_statement(resource_handle, name)?
            }
            SessionCommand::CommitSessionTransaction => {
                self.commit_transaction(resource_handle, handle_id)?
            }
            SessionCommand::RollbackSessionTransaction => {
                self.rollback_transaction(resource_handle, handle_id)?
            }
            SessionCommand::CloseSessionCursor => self.close_cursor(resource_handle, handle_id)?,
            // 下面三条是 §9.2 的**反例命令**，它们存在的目的就是被判负。
            SessionCommand::BeginSessionTransactionUnregistered => {
                self.begin_transaction_unregistered(resource_handle)?
            }
            SessionCommand::CommitWithStaleHandle => {
                let stale_epoch = input
                    .get("runtimeEpoch")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| {
                        GatewayError::InvalidInput("runtimeEpoch 必须是整数".to_owned())
                    })?;
                self.commit_with_stale_epoch(
                    resource_handle,
                    handle_id,
                    crate::connection::types::Counter::new(stale_epoch),
                )?
            }
            SessionCommand::HandleFromOtherResource => {
                let other = input
                    .get("resourceId")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| {
                        GatewayError::InvalidInput("resourceId 必须是字符串".to_owned())
                    })?;
                self.handle_from_other_resource(resource_handle, other, handle_id)?
            }
        };
        Ok(CommandResult::new(data))
    }

    // ---- §4.3 I1–I8 ----

    /// 泄漏不变量检查。**必须由用例显式调用**（§4.3），不用 `Drop`。
    ///
    /// 返回 `Err(消息)` 而不是直接 panic，是为了让 CM-73 这类**要对比多个检查点**的用例
    /// 能把「停住时的状态」和「收口后的状态」都拿到手再断言。
    pub fn assert_no_leak(&self) -> Result<(), String> {
        let mut violations = Vec::new();

        // I1 / I6：permit 收支平衡，且总量等于 live 资源 + 环境占用。
        self.provider.journal().assert().ledger_violations().iter().for_each(|v| {
            violations.push(format!("I1/I6 permit 台账：{v}"));
        });
        // I2：没有仍占用预算的资源。
        let live_resources = self.provider.live_resources();
        if !live_resources.is_empty() {
            violations.push(format!(
                "I2 live_resources 非空：{:?}",
                live_resources.iter().map(|r| r.as_str()).collect::<Vec<_>>()
            ));
        }
        // I3：没有未归还的 lease。
        let live_leases = self.provider.journal().live_leases();
        if !live_leases.is_empty() {
            violations.push(format!(
                "I3 live_leases 非空：{:?}",
                live_leases.iter().map(|l| l.as_str()).collect::<Vec<_>>()
            ));
        }
        // I4：没有活跃会话。
        let active_sessions = self.provider.journal().active_sessions();
        if !active_sessions.is_empty() {
            violations.push(format!(
                "I4 session_registry.active_sessions 非空：{:?}",
                active_sessions.iter().map(|s| s.as_str()).collect::<Vec<_>>()
            ));
        }
        // I5：句柄登记册必须清空。
        let registered: Vec<String> =
            self.provider.journal().handle_registry().keys().cloned().collect();
        if !registered.is_empty() {
            violations.push(format!("I5 journal.handle_registry 非空：{registered:?}"));
        }
        // I7：没有未回收的孤立句柄。
        let orphans = self.provider.journal().orphan_handles();
        if !orphans.is_empty() {
            violations.push(format!("I7 journal.orphan_handles 非空：{orphans:?}"));
        }
        // I8：流事件序号连续。
        self.provider.journal().assert().stream_violations().iter().for_each(|v| {
            violations.push(format!("I8 events.stream_sequence：{v}"));
        });
        // §5.3 变化点断言：预算记账与句柄登记的每一步都必须有证据。
        self.provider
            .journal()
            .assert()
            .change_point_violations()
            .iter()
            .for_each(|v| violations.push(format!("§5.3 {v}")));

        if violations.is_empty() {
            Ok(())
        } else {
            Err(format!("泄漏不变量（§4.3 I1–I8）不成立：\n  - {}", violations.join("\n  - ")))
        }
    }
}

/// 便捷构造：§8.1 `NS_A` 命名空间 + 指定 connection 的执行目标。
///
/// 目标值一律取自 [`crate::connection::testing::fixtures`]，用例里不许再出现硬编码字面量（§8.1 L425）。
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

/// 供 `session_cmds` 组装输出用的最小 json 帮助函数。
pub(crate) fn handle_payload(handle: &crate::connection::session::SessionHandleRef) -> JsonValue {
    json!({
        "handleId": handle.handle_id.as_str(),
        "kind": handle.kind.as_str(),
        "resourceId": handle.resource_id.as_str(),
        "runtimeEpoch": handle.runtime_epoch.get(),
    })
}
