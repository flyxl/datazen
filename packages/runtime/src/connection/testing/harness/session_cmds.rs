//! §9.1 十条会话级句柄命令的**分发实现**。
//!
//! [FakeHarness::invoke](super::FakeHarness::invoke) 负责「查表 + 校验入参」，
//! 本文件负责把校验过的 `input` 落到 [`FakeResourceProvider`] 的九个操作上，
//! 并按 `commands.rs` 声明的 output schema 组装 `CommandResult.data`。
//!
//! # 为什么不是 `DatabaseDriver`
//!
//! §9.1 说这些命令「通过 driver-api 的 `command_definitions()` / `execute_command()`
//! 通道暴露」。P0 阶段没有承载 `datazen-runtime` 夹具的 driver crate，硬造一个会
//! 越过「只加夹具与测试、不改生产执行路径、不得让 driver crate 依赖新 crate」的铁律。
//! 详见 `super` 的模块文档。这里能复用的部分（命令定义、入参校验、结果包装）
//! 全部复用 driver-api 本身，没有另写一套校验。

use std::fmt;

use serde_json::{json, Value as JsonValue};

use crate::connection::error::ProviderError;
use crate::connection::port::{ResourceHandle, TransactionOperation};
use crate::connection::session::{HandleKind, SessionHandleRef};
use crate::connection::types::{Counter, HandleId};

use super::{handle_payload, FakeHarness};

/// 命令网关的三类失败：命令不认识 / 入参不合法 / 提供方拒绝。
///
/// `Provider` 变体原样透传 [`ProviderError`]，所以 §9.2 要求的
/// `RuntimeEpochMismatch`、`SessionLost`、`RollbackFailed` 在断言里
/// 可以直接 `matches!(err, GatewayError::Provider(ProviderError::SessionLost(_)))`。
#[derive(Debug)]
pub enum GatewayError {
    UnknownCommand(String),
    InvalidInput(String),
    Provider(ProviderError),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GatewayError::UnknownCommand(id) => write!(f, "未知命令：{id}"),
            GatewayError::InvalidInput(message) => write!(f, "入参不合法：{message}"),
            GatewayError::Provider(error) => write!(f, "提供方拒绝：{error}"),
        }
    }
}

impl std::error::Error for GatewayError {}

impl GatewayError {
    /// §9.2 断言要读错误码，所以给一条直达通道。
    pub fn api_code(&self) -> Option<crate::connection::error::ApiErrorCode> {
        match self {
            GatewayError::Provider(error) => error.api_code(),
            GatewayError::UnknownCommand(_) => Some(crate::connection::error::ApiErrorCode::InvalidArgument),
            GatewayError::InvalidInput(_) => Some(crate::connection::error::ApiErrorCode::InvalidArgument),
        }
    }
}

// ---- input 取值助手 ------------------------------------------------------

/// `validate_command_input` 已经保证字段存在；这里只负责**类型**，
/// 类型不对就当成入参不合法，而不是默默取默认值。
pub(super) fn u64_field(input: &JsonValue, field: &str) -> Result<u64, GatewayError> {
    input
        .get(field)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| GatewayError::InvalidInput(format!("{field} 必须是非负整数")))
}

pub(super) fn str_field<'a>(input: &'a JsonValue, field: &str) -> Result<&'a str, GatewayError> {
    input
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| GatewayError::InvalidInput(format!("{field} 必须是字符串")))
}

pub(super) fn handle_id_field(input: &JsonValue) -> Result<HandleId, GatewayError> {
    Ok(HandleId::new(str_field(input, "handleId")?))
}

// ---- 分发实现 ------------------------------------------------------------

impl FakeHarness {
    /// 句柄 id 一律从该资源的 `dbSessionId` 派生的 `executionId` 派生
    /// （§8.2：`exe_<dbSessionId>_<seq:04>`），再挂 `hdl_` 前缀，
    /// 保证**跨资源不可能撞号**，也就不会出现「句柄跨资源/epoch 登记」。
    fn next_handle_id(&self, resource: &ResourceHandle) -> Result<(crate::connection::types::ExecutionId, HandleId), GatewayError> {
        let slot = self
            .provider()
            .resource(&resource.resource_id)
            .ok_or_else(|| GatewayError::Provider(ProviderError::SessionLost(format!(
                "资源 {} 不存在",
                resource.resource_id.as_str()
            ))))?;
        let execution_id = self.ids().next_execution_id(&slot.db_session_id);
        let handle_id = HandleId::new(format!("hdl_{}", execution_id.as_str()));
        Ok((execution_id, handle_id))
    }

    fn ensure_registered(&self, resource: &ResourceHandle, handle_id: &HandleId) -> Result<(), GatewayError> {
        let owner = self
            .journal()
            .handle_registry()
            .get(handle_id.as_str())
            .map(|record| record.resource_id.clone());
        match owner {
            Some(owner) if owner == resource.resource_id => Ok(()),
            Some(owner) => Err(GatewayError::Provider(ProviderError::SessionLost(format!(
                "句柄 {} 登记在资源 {} 上，不能在资源 {} 上复用",
                handle_id.as_str(),
                owner.as_str(),
                resource.resource_id.as_str()
            )))),
            None => Err(GatewayError::Provider(ProviderError::SessionNotFound(format!(
                "句柄 {} 未登记",
                handle_id.as_str()
            )))),
        }
    }

    /// §9.1 `begin_session_transaction` / `begin_session_transaction_hold`（入参 `{}`）。
    ///
    /// 两条命令走同一段实现 —— `hold` 版本已经在 [`FakeHarness::invoke`] 里推进过假时钟，
    /// 且**不自动终结事务**。
    fn begin_transaction(&self, resource: &ResourceHandle) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        self.provider()
            .transaction_operation(resource, TransactionOperation::Begin)
            .map_err(GatewayError::Provider)?;
        let (_execution_id, handle_id) = self.next_handle_id(resource)?;
        let handle = self
            .provider()
            .register_handle(
                &resource.resource_id,
                HandleKind::Transaction,
                handle_id,
                None,
            )
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "sessionHandles": [handle_payload(&handle)],
        }))
    }

    /// §9.1 `open_session_cursor`（入参 `{ rows }`）。
    ///
    /// 游标句柄**必须显式关闭**：没有 `rows` 支撑就不回收，§9.1 的 5 分钟空闲事务规则由此可测。
    /// 这里把 `rows` 记进输出，好让用例断言「游标开了但没关 → I5 不成立」。
    fn open_cursor(&self, resource: &ResourceHandle, rows: u64) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        let (_execution_id, handle_id) = self.next_handle_id(resource)?;
        let handle = self
            .provider()
            .register_handle(&resource.resource_id, HandleKind::Cursor, handle_id, None)
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "rows": rows,
            "sessionHandles": [handle_payload(&handle)],
        }))
    }

    /// §9.1 `prepare_server_statement`（入参 `{ name }`）。
    fn prepare_server_statement(
        &self,
        resource: &ResourceHandle,
        name: &str,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        let (_execution_id, handle_id) = self.next_handle_id(resource)?;
        let handle = self
            .provider()
            .register_handle(
                &resource.resource_id,
                HandleKind::ServerPrepared,
                handle_id,
                None,
            )
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "name": name,
            "sessionHandles": [handle_payload(&handle)],
        }))
    }

    /// §9.1 `commit_session_transaction`（入参 `{ handleId }`）。
    ///
    /// §9.2：注入 F9 时 `effectOutcome` 必须是 `unknown`、`errorCode` 必须是**实际原因**
    /// （`protocolError` / `timeout`），不得自动重放、不得抛 `TransactionResolutionRequired`。
    /// 句柄登记**保留**：提交不可判定时句柄最终归属未知（§5.3 规则 6）。
    fn commit_transaction(
        &self,
        resource: &ResourceHandle,
        handle_id: &HandleId,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        self.ensure_registered(resource, handle_id)?;
        let completion = self
            .provider()
            .transaction_operation(resource, TransactionOperation::Commit(handle_id.clone()))
            .map_err(GatewayError::Provider)?;
        Ok(terminal_payload(&completion, handle_id, self))
    }

    /// §9.1 `rollback_session_transaction`（入参 `{ handleId }`）。
    ///
    /// §9.2：注入 F10 时资源进 `Quarantined`、**预算占用保留** —— 错误以
    /// [`ProviderError::RollbackFailed`] 返回，`Quarantined` 事件已写进台账。
    fn rollback_transaction(
        &self,
        resource: &ResourceHandle,
        handle_id: &HandleId,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        self.ensure_registered(resource, handle_id)?;
        let completion = self
            .provider()
            .transaction_operation(resource, TransactionOperation::Rollback(handle_id.clone()))
            .map_err(GatewayError::Provider)?;
        Ok(terminal_payload(&completion, handle_id, self))
    }

    /// §9.1 `close_session_cursor`（入参 `{ handleId }`）。
    fn close_cursor(
        &self,
        resource: &ResourceHandle,
        handle_id: &HandleId,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        let handle = self
            .provider()
            .close_handle(
                &resource.resource_id,
                handle_id,
                "§9.1 close_session_cursor：显式关闭游标",
            )
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "sessionHandles": [handle_payload(&handle)],
        }))
    }

    /// §9.1 `begin_session_transaction_unregistered`（入参 `{}`，**反例**）。
    ///
    /// 造出一个事务句柄却**不**写 `sessionHandles` 登记册；台账里只留一条 `orphaned`。
    /// I7 断言关资源时孤儿句柄被回收。
    fn begin_transaction_unregistered(
        &self,
        resource: &ResourceHandle,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        self.provider()
            .transaction_operation(resource, TransactionOperation::Begin)
            .map_err(GatewayError::Provider)?;
        let (_execution_id, handle_id) = self.next_handle_id(resource)?;
        let handle = self
            .provider()
            .orphan_handle(&resource.resource_id, HandleKind::Transaction, handle_id)
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "sessionHandles": [handle_payload(&handle)],
        }))
    }

    /// §9.1 `commit_with_stale_handle`（入参 `{ handleId, runtimeEpoch }`，**反例**）。
    ///
    /// §9.2：拿**过期 epoch** 的凭证提交 → 判负 `RuntimeEpochMismatch`，
    /// 且**原句柄状态不变**（不进事务台账、不注销登记）。
    fn commit_with_stale_epoch(
        &self,
        resource: &ResourceHandle,
        handle_id: &HandleId,
        stale_epoch: Counter,
    ) -> Result<JsonValue, GatewayError> {
        let stale = ResourceHandle {
            resource_id: resource.resource_id.clone(),
            runtime_epoch: stale_epoch,
            owner_token: resource.owner_token.clone(),
        };
        self.provider()
            .transaction_operation(&stale, TransactionOperation::Commit(handle_id.clone()))
            .map_err(GatewayError::Provider)?;
        Ok(json!({
            "effectOutcome": crate::connection::EffectOutcome::Completed.as_str(),
            "sessionHandles": [],
        }))
    }

    /// §9.1 `handle_from_other_resource`（入参 `{ handleId, resourceId }`，**反例**）。
    ///
    /// §9.2：拿别的资源上的句柄在当前资源上提交 → 判负 `SessionLost`。
    /// 判定放在网关：台账里 `handleId` 的归属资源与出示凭证的资源不是同一个。
    fn handle_from_other_resource(
        &self,
        resource: &ResourceHandle,
        other_resource_id: &str,
        handle_id: &HandleId,
    ) -> Result<JsonValue, GatewayError> {
        self.provider()
            .verify_handle(resource)
            .map_err(GatewayError::Provider)?;
        let claimed = self
            .journal()
            .handle_registry()
            .get(handle_id.as_str())
            .map(|record| record.resource_id.as_str().to_owned());
        match claimed {
            Some(owner) if owner != other_resource_id => {
                return Err(GatewayError::Provider(ProviderError::SessionLost(format!(
                    "句柄 {} 登记在资源 {} 上，与入参声称的 {other_resource_id} 不符",
                    handle_id.as_str(),
                    owner
                ))))
            }
            Some(owner) => {
                return Err(GatewayError::Provider(ProviderError::SessionLost(format!(
                    "句柄 {} 属于资源 {owner}，不能在资源 {} 上复用",
                    handle_id.as_str(),
                    resource.resource_id.as_str()
                ))))
            }
            None => {
                return Err(GatewayError::Provider(ProviderError::SessionLost(format!(
                    "句柄 {} 未登记，无法确认归属资源",
                    handle_id.as_str()
                ))))
            }
        }
    }
}

/// 终态命令（提交 / 回滚）的输出形状：`effectOutcome` + 可选 `errorCode`。
///
/// 保留句柄时一并回带 `sessionHandles` —— F9 之后句柄**仍在**登记册里（§5.3 规则 6），
/// 用例需要能直接看到这个事实，而不是去翻台账。
fn terminal_payload(
    completion: &crate::connection::port::ExecutionCompletion,
    handle_id: &HandleId,
    harness: &FakeHarness,
) -> JsonValue {
    let open: Vec<JsonValue> = harness
        .journal()
        .handle_registry()
        .get(handle_id.as_str())
        .filter(|record| !record.closed)
        .map(|record| {
            handle_payload(&SessionHandleRef::new(
                handle_id.clone(),
                record.kind,
                record.resource_id.clone(),
                record.runtime_epoch,
            ))
        })
        .collect();
    let mut data = json!({
        "effectOutcome": completion.effect_outcome.as_str(),
        "sessionHandles": open,
    });
    if let Some(code) = completion.error_code {
        data["errorCode"] = JsonValue::String(code.as_str().to_owned());
    } else {
        data["errorCode"] = JsonValue::Null;
    }
    data
}