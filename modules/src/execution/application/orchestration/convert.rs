use crate::connection::domain::NamespaceTarget;
use crate::execution as pa;
use crate::execution::domain::ExecutionTarget as PaExecutionTarget;
use crate::execution::runtime::connection as rt;
use crate::execution::runtime::registry::epoch::epoch_string;
use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::error::{ApiError, ApiErrorCode};

pub(super) fn namespace(value: &NamespaceTarget) -> Result<rt::NamespaceTarget, ApiError> {
    // The legacy kernel scalar path cannot represent arbitrary hierarchical IDs losslessly.
    if value.path.len() > 1 {
        return Err(ApiError::new(
            ApiErrorCode::TargetUnsupported,
            "multi-segment namespace is not supported by this resource kernel",
        ));
    }
    Ok(rt::NamespaceTarget {
        database: value.database.clone().unwrap_or_default(),
        catalog: value.catalog.clone().unwrap_or_default(),
        schema: value.schema.clone().unwrap_or_default(),
        path: value.path.first().cloned().unwrap_or_default(),
    })
}
pub(super) fn public_namespace(value: &rt::NamespaceTarget) -> NamespaceTarget {
    let optional = |v: &str| {
        if v.is_empty() || v == rt::types::UNKNOWN_SENTINEL {
            None
        } else {
            Some(v.to_owned())
        }
    };
    NamespaceTarget::new(
        optional(&value.database),
        optional(&value.catalog),
        optional(&value.schema),
        optional(&value.path).into_iter().collect(),
    )
}
pub(super) fn target(value: &PaExecutionTarget) -> Result<rt::ExecutionTarget, ApiError> {
    Ok(rt::ExecutionTarget {
        connection_id: value.connection_id.clone(),
        namespace: namespace(&value.namespace)?,
        object: value.object.as_ref().map(|o| rt::ObjectTarget {
            kind: o.kind.clone(),
            name: o.name.clone(),
            signature: o.signature.clone().unwrap_or_default(),
        }),
    })
}
pub(super) fn handle(value: &pa::SessionHandle) -> Result<rt::SessionHandle, ApiError> {
    let epoch = value
        .runtime_epoch
        .as_str()
        .strip_prefix("rte-")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| ApiError::new(ApiErrorCode::SessionLost, "invalid runtime epoch"))?;
    Ok(rt::SessionHandle {
        db_session_id: value.db_session_id.clone(),
        runtime_epoch: rt::Counter::new(epoch),
    })
}
pub(super) fn public_handle(value: &rt::SessionHandle) -> pa::SessionHandle {
    pa::SessionHandle::new(
        value.db_session_id.clone(),
        crate::shared::id::RuntimeEpoch::new(epoch_string(value.runtime_epoch.get())),
    )
}
pub(super) fn context(value: &rt::SessionContext) -> pa::SessionContext {
    pa::SessionContext {
        namespace: public_namespace(&value.namespace),
        search_path: Some(value.search_path.clone()),
        effective_identity: (!value.effective_identity.is_empty())
            .then(|| value.effective_identity.clone()),
        transaction_state: match value.transaction_state {
            rt::TransactionState::None => pa::TransactionState::None,
            rt::TransactionState::Active => pa::TransactionState::Active,
            rt::TransactionState::Aborted => pa::TransactionState::Aborted,
            rt::TransactionState::Unknown => pa::TransactionState::Unknown,
            rt::TransactionState::Unsupported => pa::TransactionState::Unsupported,
        },
        autocommit: Some(value.autocommit),
        confidence: match value.confidence {
            rt::ContextConfidence::Confirmed => pa::ContextConfidence::Confirmed,
            rt::ContextConfidence::Partial => pa::ContextConfidence::Partial,
            rt::ContextConfidence::Unknown => pa::ContextConfidence::Unknown,
        },
    }
}
pub(super) fn owner(
    ctx: &RequestContext,
    connection: &rt::ConnectionId,
    value: &OwnerRef,
) -> Result<rt::OwnerRef, ApiError> {
    crate::shared::policy::check_owner(ctx, value, None)?;
    match value {
        OwnerRef::Editor {
            client_instance_id,
            editor_session_id,
        } => Ok(rt::OwnerRef::Editor {
            organization_id: ctx.organization_id.clone(),
            principal_id: ctx.principal_id.clone(),
            connection_id: connection.clone(),
            client_instance_id: client_instance_id.clone(),
            editor_session_id: editor_session_id.clone(),
        }),
        OwnerRef::ClientSession {
            client_instance_id, ..
        } => Ok(rt::OwnerRef::ClientSession {
            client_instance_id: client_instance_id.clone(),
        }),
        _ => Err(ApiError::permission_denied(
            "job ownership must be established by the job runtime",
        )),
    }
}
pub(super) fn session(
    value: rt::SessionView,
    owner: OwnerRef,
    initial: PaExecutionTarget,
    attached: bool,
) -> pa::SessionView {
    pa::SessionView {
        handle: public_handle(&value.handle),
        connection_id: value.connection_id,
        config_revision: rt::Counter::new(value.config_revision.get()),
        owner,
        initial_target: initial,
        observed_context: context(&value.observed_context),
        context_revision: value.context_revision,
        state: match value.state {
            rt::SessionState::New => pa::SessionState::New,
            rt::SessionState::Opening => pa::SessionState::Opening,
            rt::SessionState::Ready => pa::SessionState::Ready,
            rt::SessionState::Executing => pa::SessionState::Executing,
            rt::SessionState::Reconfiguring => pa::SessionState::Reconfiguring,
            rt::SessionState::Closing => pa::SessionState::Closing,
            rt::SessionState::Closed => pa::SessionState::Closed,
            rt::SessionState::Lost => pa::SessionState::Lost,
        },
        attachment_state: if attached {
            pa::AttachmentState::Attached
        } else {
            pa::AttachmentState::Detached
        },
        active_execution_id: value.active_execution_id,
        expires_at: None,
    }
}
pub(super) fn runtime(error: rt::RuntimeError) -> ApiError {
    // The admission refusal already carries the two numbers the user can act
    // on. Keep every other runtime failure collapsed into the fixed category
    // text: their payloads may carry identifiers and are not caller-facing.
    if let rt::RuntimeError::PhysicalCapacityExceeded { limit, used } = &error {
        return ApiError::new(
            ApiErrorCode::ResourceBusy,
            format!("连接数已达上限（上限 {limit}，当前 {used}），请关闭部分连接后重试"),
        );
    }
    ApiError::new(
        error.api_code().unwrap_or(ApiErrorCode::OutcomeUnknown),
        "connection runtime operation failed",
    )
}
/// 活连接出口端口的错误映射。
///
/// 消息是**给调用方看的分类**，不是内部细节：端口侧的驱动报错文本可能带连接串，
/// 因此不外传，一律收敛成固定文案（与 [`port`] / [`runtime`] 同一纪律）。
pub(super) fn live(error: super::live::LiveConnectionError) -> ApiError {
    use super::live::LiveConnectionError;
    let code = match error {
        LiveConnectionError::SessionLost(_) => ApiErrorCode::SessionLost,
        LiveConnectionError::RuntimeEpochMismatch => ApiErrorCode::SessionLost,
        LiveConnectionError::Revoked => ApiErrorCode::SessionLost,
        LiveConnectionError::NotWired | LiveConnectionError::Refused(_) => {
            ApiErrorCode::ServiceUnavailable
        }
    };
    ApiError::new(code, "live connection port operation failed")
}
pub(super) fn port(error: crate::shared::error::PortError) -> ApiError {
    use crate::shared::error::PortError;
    let code = match error {
        PortError::NotFound(_) => ApiErrorCode::NotFound,
        PortError::CasConflict { .. } => ApiErrorCode::ContextConflict,
        _ => ApiErrorCode::ServiceUnavailable,
    };
    ApiError::new(code, "connection service operation failed")
}
pub(super) fn json_convert<T: serde::de::DeserializeOwned, S: serde::Serialize>(
    value: S,
) -> Result<T, ApiError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|_| {
            ApiError::new(
                ApiErrorCode::OutcomeUnknown,
                "incompatible runtime projection",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_capacity_refusal_keeps_its_actionable_numbers() {
        let mapped = runtime(rt::RuntimeError::PhysicalCapacityExceeded {
            limit: 16,
            used: 16,
        });
        assert_eq!(mapped.code, ApiErrorCode::ResourceBusy);
        assert_eq!(
            mapped.message,
            "连接数已达上限（上限 16，当前 16），请关闭部分连接后重试"
        );
    }

    #[test]
    fn other_runtime_failures_stay_redacted_category_text() {
        let mapped = runtime(rt::RuntimeError::BudgetExhausted("resourceOpenBusy"));
        assert_eq!(mapped.message, "connection runtime operation failed");
    }
}
