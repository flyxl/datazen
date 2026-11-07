//! L6：授权（team-server §6.1 求值顺序 + §7.1 的 HTTP 映射）。
//!
//! 求值顺序（§6.1，任一步终止）在 [`crate::policy`]；本层是它的 HTTP
//! 形态，并落地 §7.1 的映射：
//!
//! | Decision | 路由形态 | HTTP | code |
//! | --- | --- | --- | --- |
//! | `NotVisible` | 任意 | 404 | `NotFound`（固定文案，不含资源信息） |
//! | `Denied` | `{id}` 寻址 | 404 | `NotFound`（与 NotVisible 完全一致） |
//! | `Denied` | 组织级动作 | 403 | `PermissionDenied` |
//! | `Allowed` | 放行 | — | — |
//!
//! 「可见性先于动作」由 [`crate::policy::PolicyService::authorize`] 的
//! 组织边界检查保证（越组织资源在 DB 查询前就 `NotVisible`）；本层构造
//! 请求时把身份限定为 L2 注入的会话事实（INV-S1）。
//!
//! ## 本刀的探针形态（重要）
//!
//! L6 中间件只对**已登记形状**的路径求值；未登记的 `/api/` 路径原样
//! 放行到 fallback（骨架的 501）——每一域的精确 action 语义（如 session
//! 的 owner 继承、execution 的 cancel）随该域 handler 切片在此登记，
//! 不在路由未点亮时发明语义。已登记形状：
//!
//! - `/api/v1/connections/{id}`：`ResourceType::Connection`，
//!   `GET/HEAD → Read`、`PATCH → Write`、`DELETE → Write`。
//! - `/api/v1/executions/{id}`：`ResourceType::Execution`，
//!   `GET/HEAD → Read`（rbac 基线矩阵裁决——OrgMember/JobOperator/owner
//!   系可读，ConnectionViewer/auditor 语义见矩阵）。三段 `/cancel` 子动作
//!   与集合级提交不入两层段表（action 语义在用例层）。
//!
//! 集合类路由（无 id）不在此求值（§3.3：集合类只返回可见集合，组织级
//! 动作权限不足才 403——集合可见性过滤是 handler 的职责）。
//!
//! 已授权 owner 的会话存活与 `runtimeEpoch` 复核（§3.2 L6 行的 409 分支）
//! 需要 `SessionDirectory`（WR-D4），随执行域 handler 落地。

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::RequestId;

use crate::http::middleware::auth::AuthenticatedSession;
use crate::http::middleware::{auth_exempt, under_api, AuthChainState};
use crate::http::response::ApiErrorResponse;
use crate::policy::acl::Decision;
use crate::policy::{Action, AuthorizeRequest, ResourceRef, ResourceType};

/// §7.1 的固定 404 文案：`NotVisible` / `Denied`（id 寻址）与「资源确实
/// 不存在」三者共用同一条——不含资源名、类型、归属、大小、时间戳，
/// 防 ID 枚举。
pub const NOT_VISIBLE_MESSAGE: &str =
    "The addressed resource does not exist or is not visible to you.";

/// L6 中间件本体。挂载点：`build_router_with_auth`，L4 之内、handler 之外。
pub(crate) async fn guard(
    State(state): State<AuthChainState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();
    if !under_api(&path) || auth_exempt(&path) {
        return next.run(req).await;
    }

    // L2 已放行才可能到这里；扩展缺失 = 装配错误，fail-closed 拒绝。
    let Some(session) = req.extensions().get::<AuthenticatedSession>().cloned() else {
        return not_found(req.extensions().get::<RequestId>().cloned());
    };
    let request_id = req.extensions().get::<RequestId>().cloned();

    let Some((resource_type, resource_id)) = resource_ref_from_path(&path) else {
        // 未登记形状：不在本层发明语义，交给 fallback（骨架 501）。
        return next.run(req).await;
    };
    let Some(action) = action_for_method(req.method()) else {
        // 未登记方法（POST 子动作等）同上。
        return next.run(req).await;
    };

    let request = AuthorizeRequest {
        organization_id: session.organization_id.clone(),
        principal_id: session.principal_id.clone(),
        resource: ResourceRef {
            organization_id: session.organization_id.clone(),
            resource_type,
            resource_id,
        },
        action,
    };
    match state.policy.authorize(&request) {
        Ok(Decision::Allowed) => next.run(req).await,
        Ok(Decision::NotVisible) => not_found(request_id),
        // id 寻址的 Denied 与 NotVisible 同为 404（§7.1）——组织级动作
        // （resource_id 为空串）才走 403 分支；本层已登记形状都是 id 寻址。
        Ok(Decision::Denied) => not_found(request_id),
        Err(source) => {
            // 管理库不可达：fail-closed，但如实映射 503（§9.2 的
            // ServiceUnavailable 行），不伪装成授权拒绝。
            tracing::warn!(%source, "policy evaluation failed on storage");
            service_unavailable(request_id)
        }
    }
}

/// 路径 → 资源引用（本刀的已登记形状，见模块头）。返回 `(类型, 资源id)`。
fn resource_ref_from_path(path: &str) -> Option<(ResourceType, String)> {
    let rest = path.strip_prefix("/api/v1/")?;
    let mut segments = rest.split('/');
    let collection = segments.next()?;
    let id = segments.next()?;
    // 尾随段（`{id}/sub-action`）不归本层——子动作的 action 语义随域切片登记。
    if segments.next().is_some() || id.is_empty() {
        return None;
    }
    let resource_type = match collection {
        "connections" => ResourceType::Connection,
        // W8 刀1对账（PR #204 增量）：execution 域两段形状
        // （`GET /api/v1/executions/{id}` → Read；rbac 基线矩阵裁决）。
        // 取消子动作（三段）与集合级提交不入两层段表（action 语义在
        // 用例层）。
        "executions" => ResourceType::Execution,
        // 其余域路由随各自 handler 切片登记（不发明语义）。
        _ => return None,
    };
    Some((resource_type, id.to_owned()))
}

/// 方法 → 动作（本层已登记形状的方法映射）。
fn action_for_method(method: &Method) -> Option<Action> {
    match *method {
        Method::GET | Method::HEAD => Some(Action::Read),
        Method::PATCH | Method::DELETE => Some(Action::Write),
        // POST 子动作/创建的 action 语义随域切片登记（不发明）。
        _ => None,
    }
}

/// §7.1 的 404：`NotFound` + 固定文案（与 `NotVisible` / 不存在完全一致）。
fn not_found(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE).with_request_id(request_id),
    )
    .into_response()
}

/// 503：管理库不可达（fail-closed 如实表达，不伪装成授权拒绝）。
fn service_unavailable(request_id: Option<RequestId>) -> Response {
    ApiErrorResponse::new(
        ApiError::new(
            ApiErrorCode::ServiceUnavailable,
            "The service is temporarily unavailable.",
        )
        .with_request_id(request_id),
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    /// 路径解析：已登记形状精确匹配；子动作、空 id、未登记集合都不求值。
    #[test]
    fn path_parsing_covers_only_registered_shapes() {
        assert_eq!(
            resource_ref_from_path("/api/v1/connections/c-1"),
            Some((ResourceType::Connection, "c-1".into()))
        );
        // 子动作不归本层。
        assert_eq!(resource_ref_from_path("/api/v1/connections/c-1/sub"), None);
        // 空 id（尾斜杠）不算寻址。
        assert_eq!(resource_ref_from_path("/api/v1/connections/"), None);
        // 集合类路由不在此求值。
        assert_eq!(resource_ref_from_path("/api/v1/connections"), None);
        // 未登记的集合（session/job/… 随各自切片）。
        assert_eq!(resource_ref_from_path("/api/v1/sessions/s-1"), None);
        assert_eq!(resource_ref_from_path("/api/v1/events/stream-1"), None);
        // execution 两段形状已登记（W8 刀1对账）：Read 走 L6 求值。
        assert_eq!(
            resource_ref_from_path("/api/v1/executions/exe-1"),
            Some((ResourceType::Execution, "exe-1".into()))
        );
        // 取消子动作（三段）与集合级不入本层。
        assert_eq!(
            resource_ref_from_path("/api/v1/executions/exe-1/cancel"),
            None
        );
        assert_eq!(resource_ref_from_path("/api/v1/executions"), None);
    }

    /// 方法映射：读/写二值；POST 子动作留给域切片。
    #[test]
    fn method_mapping_is_read_or_write_only() {
        assert_eq!(action_for_method(&Method::GET), Some(Action::Read));
        assert_eq!(action_for_method(&Method::HEAD), Some(Action::Read));
        assert_eq!(action_for_method(&Method::PATCH), Some(Action::Write));
        assert_eq!(action_for_method(&Method::DELETE), Some(Action::Write));
        assert_eq!(action_for_method(&Method::POST), None);
        assert_eq!(action_for_method(&Method::PUT), None);
    }

    /// §7.1 的固定 404 文案：不含资源信息。
    #[test]
    fn the_not_found_message_leaks_nothing_about_the_resource() {
        assert!(!NOT_VISIBLE_MESSAGE.contains("connection"));
        assert!(!NOT_VISIBLE_MESSAGE.contains("org"));
        assert!(!NOT_VISIBLE_MESSAGE.contains("id"));
    }

    /// 404 的 wire 形状：`NotFound` → 404（§9.2 映射表）。
    #[test]
    fn the_not_found_response_is_a_404() {
        let response = not_found(Some(RequestId::new("req-9")));
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
