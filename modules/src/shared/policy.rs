//! 归属校验的**共享纯函数**：`OwnerRef` 是否属于当前请求上下文（§5.1(3)、INV-01 的配套）。
//!
//! 住在 platform-api 而不是 application：实现用例端口的 `runtime` 也要调用它
//! （`application/convert.rs` 的请求转换），而 runtime 不得依赖 application（R2）。
//! 函数只吃契约类型（`RequestContext` / `OwnerRef` / `JobId`）并返回 `ApiError`，
//! 无状态、无端口、无 I/O——因此是契约层可以承载的**不变量**，不是编排。
//!
//! `application::identity_shared::policy::IdentityPolicy::check_owner` 保留同名入口并委托到本函数，
//! 宿主的既有调用点因此逐字不变。

use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::error::ApiError;
use crate::shared::id::JobId;

/// §5.1(3)：`OwnerRef` 可采信性。
///
/// * `Editor` / `ClientSession`：归属的 `clientInstanceId` 必须是**当前**客户端实例。
/// * `Job` / `WorkflowBlock`：归属 Job 必须等于 `authorized_job`。
///
/// 失败一律是 `PermissionDenied`——不给「是别人的」这种可用于探测的信息。
///
/// # 契约债（不要把本函数当成网关归属闸门的兜底）
///
/// 1. **生产调用点只有两处，且都只比 `clientInstanceId`**：
///    `src-tauri/src/mcp/tools.rs`（MCP 会话归属）与
///    `modules/src/execution/application/convert.rs`（请求转换，`authorized_job` 传 `None`
///    ⇒ `Job` / `WorkflowBlock` 变体一律拒绝）。「上层已经挡住了」的结论仍不成立。
/// 2. **这里的 `OwnerRef` 与 `modules` 里那个同名类型不是同一个东西。**
///    本侧是 `datazen_modules::shared::context::OwnerRef`：它的 `Editor` 只有
///    `client_instance_id` / `editor_session_id`，`Job` 只有 `job_id` / `stage_id`
///    （且 `stage_id` 是 `crate::shared::id::StageId`，网关侧同名字段是 `String`）——**两侧都没有
///    `organization_id` / `principal_id`**。因此本函数在结构上**看不到组织与主体**，
///    也不可能校验网关那一侧。
/// 3. **两侧没有任何交叉校验**，同一个变体名、同一批字段名，但语义可以各改各的、互不报警。
/// 4. 即便接上调用方，这里比的是「调用方显式传入的 `authorized_job` 与 `owner.job_id`
///    是否相等」，这是**这个 job 有没有被授权**，不是**这是不是同一个人**——与
///    CM-06「U1 指向 U2 的 job」的跨用户语义不是同一件事，不能互相顶替。
///
/// 两条同名类型由网关侧 `modules/src/execution/runtime/gateway/owner_binding.rs` 的模块头
/// 同样登记在案。统一它们是跨 crate 的契约改动，不在网关轨内单方面做。
pub fn check_owner(
    ctx: &RequestContext,
    owner: &OwnerRef,
    authorized_job: Option<&JobId>,
) -> Result<(), ApiError> {
    match owner {
        OwnerRef::Editor {
            client_instance_id,
            editor_session_id: _,
        } => {
            if client_instance_id == &ctx.client_instance_id {
                return Ok(());
            }
            Err(ApiError::permission_denied(
                "编辑器会话不属于当前客户端实例",
            ))
        }
        OwnerRef::ClientSession {
            client_instance_id,
            purpose: _,
        } => {
            if client_instance_id == &ctx.client_instance_id {
                return Ok(());
            }
            Err(ApiError::permission_denied(
                "长驻客户端会话不属于当前客户端实例",
            ))
        }
        OwnerRef::Job {
            job_id,
            stage_id: _,
        } => match authorized_job {
            Some(job) if job == job_id => Ok(()),
            _ => Err(ApiError::permission_denied("归属的 Job 未获授权")),
        },
        OwnerRef::WorkflowBlock {
            job_id,
            block_id: _,
        } => match authorized_job {
            Some(job) if job == job_id => Ok(()),
            _ => Err(ApiError::permission_denied("归属的 Job 未获授权")),
        },
    }
}
