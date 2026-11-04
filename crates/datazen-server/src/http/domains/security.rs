//! security 域路由（WR-F1#2 的 HTTP 半边；TS 半边 =
//! `packages/backend-client/src/securityClient.ts`，路径/体/响应逐字冻结）。
//!
//! | 方法 | 路由 | 令牌 | 成功 | 请求 → 响应 |
//! | --- | --- | --- | --- | --- |
//! | `security.resolveSecret` | `POST /api/v1/secrets/resolve` | — | 200 | `{domain,key}` → `{value}` |
//! | `security.putSecret` | `PUT /api/v1/secrets/{domain}/{key}` | — | 204 | `{value}` → 空体 |
//!
//! 域裁决（与 securityClient.ts 模块头逐条对照）：
//!
//! - **红线（本域的第一裁决）**：value 明文只在请求/响应体各出现**一次**
//!   ——一个 resolve 答案、一个 put 体。明文不进日志、不进错误文案、不进
//!   审计（审计只记 `{domain,key}` 坐标与动作，见下）；本文件也没有任何
//!   list/枚举面。存储半边是 WR-D4 的 [`ServerSecretVault`]：行内只有密文。
//! - **登录即可（v1）**：resolve/put 都要求 `SessionIdentity`（L2 认证链）。
//!   RBAC 细粒度授权档位随连接管理 §4.1 的凭据读写切片收紧（v1 登记：
//!   凭据读写授权 = 登录态，动作级收紧不做静默放宽）。
//! - **404 防枚举**：无权/缺失/越组织共用同一条固定文案
//!   （[`NOT_VISIBLE_MESSAGE`]，与 L6 / connections 同一条），wire 上
//!   「不存在」与「不可见」不可区分。
//! - **引用坐标**：`{domain, key}` 是前端唯一允许持有的秘密坐标；服务端把它
//!   铸成 vault 引用 `server-vault:secret:{organization_id}:{domain}/{key}`
//!   （`ServerSecretVault::mint_ref` 的 kind = `secret`）。domain/key 段做
//!   `%`/`:`/`/` 三字符转义后拼接，保证不同 (domain, key) 对不出同一引用
//!   （`ref_organization` 按 `:` 切三段，id 段内容自由）。
//! - **写路径**：vault 端口只有读面；写入组合本适配器的固有方法
//!   `ServerSecretVault::sync_store_material`（`spawn_blocking` 包装与
//!   trait 实现同款）。每次 put 分配新 revision，resolve 只答当前版本——
//!   「二次 put 覆盖」的语义由当前版本指针承担。
//! - **审计**：读/写都落一条 [`AuditEvent`]（动作 `secret_resolve` /
//!   `secret_store`，开放词表随本切片登记）。坐标投影进
//!   `payload.request.route`：put 的 wire 路径本就携带编码后的两段；resolve
//!   的体坐标投影成同一规范形式 `/api/v1/secrets/{domain}/{key}`（原值，
//!   非编码），使两类行可按引用对账。**value 明文永不进审计**；请求体校验
//!   失败（400，未触 vault）不落审计；审计写失败不吞业务响应，只告警
//!   （§3.2 同款处置）。resolve 落 `Failed` 的两种情形：404（error_code
//!   `notFound`，探测行为在账）与存储故障（`serviceUnavailable`）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use axum::{Json, Router};
use serde_json::{json, Value};

use datazen::execution::application::secret::{SecretProvider, SecretPurpose};
use datazen::shared::audit::{
    AuditEvent, AuditOutcome, AuditPayload, AuditPrincipalKind, AuditRequestContext, AuditSink,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{SecretRef, Timestamp};

use crate::execution::secret_provider::ServerSecretVault;
use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::storage::ServerAudit;

/// 本域铸造的 vault 引用 kind（`server-vault:secret:{org}:{id}`）。
const REF_KIND: &str = "secret";

/// resolve 的取材用途（`SecretPurpose::Custom` 的登记字面量；v1 purpose
/// 不参与字段切分，见 secret_provider.rs 模块头）。
const RESOLVE_PURPOSE: &str = "security.resolveSecret";

/// security 域的装配状态（composition root 注入；无隐式单例）。
#[derive(Clone)]
pub struct SecurityState {
    /// 版本化秘密保险库（WR-D4 适配器；行内只有密文）。
    pub vault: Arc<ServerSecretVault>,
    /// 审计追加写（读/写都记账；append-only）。
    pub audit: Arc<ServerAudit>,
}

/// security 域 Router 片段：`POST /api/v1/secrets/resolve` 与
/// `PUT /api/v1/secrets/{domain}/{key}`。主线把它 merge 进
/// `build_router_with_auth` 的认证链（L2–L6 之内）。
pub fn security_router(state: SecurityState) -> Router {
    Router::new()
        .route("/api/v1/secrets/resolve", post(handler_resolve_secret))
        .route("/api/v1/secrets/{domain}/{key}", put(handler_put_secret))
        .with_state(state)
}

// ── 引用坐标 ────────────────────────────────────────────────────────────────

/// `{domain}/{key}` 拼接段的转义：`%`、`:`、`/` 三个字符 `%XX` 化。
///
/// `%` 先行转义保证转义不可再解释（输出是单射的）：不同 (domain, key) 对
/// 铸不出同一段；`:` 的转义让 id 段不与 `ref_organization` 的三段切分冲突；
/// `/` 的转义让两段边界唯一。其余字符（含非 ASCII）原样通过。
fn escape_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '%' | ':' | '/' => out.push_str(&format!("%{:02X}", ch as u32)),
            _ => out.push(ch),
        }
    }
    out
}

/// 会话组织 + (domain, key) → vault 引用。组织段来自认证链事实（INV-01），
/// 越组织解析在 vault 的引用解析层就被拒（`NotFound`，CM-05 不可区分）。
fn secret_ref_for(organization_id: &str, domain: &str, key: &str) -> SecretRef {
    let id = format!("{}/{}", escape_segment(domain), escape_segment(key));
    ServerSecretVault::mint_ref(REF_KIND, organization_id, &id)
}

/// resolve 请求体的坐标（TS 半边 `requireNonEmpty` 的服务端镜像：两段都是
/// 非空字符串）。
fn resolve_coordinates(body: &Value) -> Option<(String, String)> {
    let domain = body.get("domain").and_then(Value::as_str)?;
    let key = body.get("key").and_then(Value::as_str)?;
    if domain.is_empty() || key.is_empty() {
        return None;
    }
    Some((domain.to_owned(), key.to_owned()))
}

// ── wire 助手 ───────────────────────────────────────────────────────────────

/// 越组织/不存在/无权的统一 404（防枚举固定文案；与 L6 同一条）。
fn not_visible() -> Response {
    api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
}

fn service_unavailable(message: impl Into<String>) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        message.into(),
    ))
}

// ── 审计 ────────────────────────────────────────────────────────────────────

/// 审计的引用坐标投影：规范资源路径形式 `/api/v1/secrets/{domain}/{key}`
/// （原值，非编码；put 的 wire 路径与它同形）。只承载 domain/key——value
/// 明文永不进审计（红线）。
fn audit_route(domain: &str, key: &str) -> String {
    format!("/api/v1/secrets/{domain}/{key}")
}

/// 落一条审计事件。写失败不吞业务响应（§3.2 同款处置）：告警后主流程继续。
fn audit_secret(
    state: &SecurityState,
    ctx: &RequestContext,
    action: &str,
    outcome: AuditOutcome,
    error_code: Option<&str>,
    domain: &str,
    key: &str,
) {
    let event = AuditEvent {
        principal_id: Some(ctx.principal_id.clone()),
        client_instance_id: Some(ctx.client_instance_id.clone()),
        delegation_id: ctx.delegation_id.clone(),
        request_id: Some(ctx.request_id.clone()),
        error_code: error_code.map(str::to_string),
        payload: AuditPayload {
            request: Some(AuditRequestContext {
                route: audit_route(domain, key),
                origin: None,
            }),
            ..AuditPayload::empty()
        },
        ..AuditEvent::new(
            ctx.organization_id.clone(),
            AuditPrincipalKind::User,
            action,
            outcome,
            Timestamp::new(now_string()),
        )
    };
    if let Err(source) = state.audit.record(event) {
        tracing::warn!(error = %source, "security audit append failed");
    }
}

fn now_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// ── handlers ────────────────────────────────────────────────────────────────

/// `security.resolveSecret`（POST /api/v1/secrets/resolve，体 `{domain,key}`）
/// → 200 `{value}`；无权/缺失 → 404（防枚举，永不 403/200-空）。
///
/// 走 vault 端口的解封读（`resolve`：无缓存，每次落库；组织隔离由引用内的
/// 组织段承载）。value 明文只在这里进响应体一次（红线）。
async fn handler_resolve_secret(
    State(state): State<SecurityState>,
    identity: SessionIdentity,
    Json(body): Json<Value>,
) -> Response {
    let Some((domain, key)) = resolve_coordinates(&body) else {
        return invalid_argument(
            "security.resolveSecret requires non-empty string domain and key.",
        );
    };
    let secret_ref = secret_ref_for(identity.ctx.organization_id.as_str(), &domain, &key);
    let resolved = state
        .vault
        .resolve(
            &identity.ctx,
            secret_ref,
            SecretPurpose::Custom(RESOLVE_PURPOSE),
        )
        .await;
    match resolved {
        Ok(material) => {
            // 材料由 put 以 JSON 字符串字节密封，非 UTF-8 只可能来自存储
            // 被篡改（完整性 MAC 已先行拒绝）——按存储故障回答，文案零明文。
            let value = match std::str::from_utf8(material.material()).map(str::to_owned) {
                Ok(value) => value,
                Err(_) => {
                    audit_secret(
                        &state,
                        &identity.ctx,
                        "secret_resolve",
                        AuditOutcome::Failed,
                        Some(ApiErrorCode::ServiceUnavailable.as_str()),
                        &domain,
                        &key,
                    );
                    return service_unavailable("secret material is not decodable text.");
                }
            };
            audit_secret(
                &state,
                &identity.ctx,
                "secret_resolve",
                AuditOutcome::Succeeded,
                None,
                &domain,
                &key,
            );
            (StatusCode::OK, Json(json!({ "value": value }))).into_response()
        }
        Err(PortError::NotFound(_)) => {
            audit_secret(
                &state,
                &identity.ctx,
                "secret_resolve",
                AuditOutcome::Failed,
                Some(ApiErrorCode::NotFound.as_str()),
                &domain,
                &key,
            );
            not_visible()
        }
        Err(source) => {
            audit_secret(
                &state,
                &identity.ctx,
                "secret_resolve",
                AuditOutcome::Failed,
                Some(ApiErrorCode::ServiceUnavailable.as_str()),
                &domain,
                &key,
            );
            service_unavailable(format!("secret resolve failed: {source}"))
        }
    }
}

/// `security.putSecret`（PUT /api/v1/secrets/{domain}/{key}，体 `{value}`）
/// → 204 空体。domain/key 进路径，绝不在体里重复（TS 半边 `putSecretBody`
/// 的镜像）；空 value 合法（TS 半边只要求 string）。
///
/// 写入走 [`ServerSecretVault::sync_store_material`]（`spawn_blocking` 包装，
/// 与 trait 读面的异步边界同款）：新 revision 分配 + 旧版本行保留，resolve
/// 只答当前版本——覆盖语义由当前版本指针承担。
async fn handler_put_secret(
    State(state): State<SecurityState>,
    identity: SessionIdentity,
    Path((domain, key)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    if domain.is_empty() || key.is_empty() {
        return invalid_argument(
            "security.putSecret requires non-empty domain and key path segments.",
        );
    }
    let Some(value) = body.get("value").and_then(Value::as_str) else {
        return invalid_argument(
            "security.putSecret requires a string value field; the plaintext travels only in this request body.",
        );
    };
    let secret_ref = secret_ref_for(identity.ctx.organization_id.as_str(), &domain, &key);
    let vault = state.vault.clone();
    let organization_id = identity.ctx.organization_id.clone();
    // 明文离开请求体的唯一去处是密封原语的入参（行内只有密文）。
    let material = value.as_bytes().to_vec();
    let stored = tokio::task::spawn_blocking(move || {
        vault.sync_store_material(organization_id.as_str(), &secret_ref, &material)
    })
    .await;
    match stored {
        Ok(Ok(_revision)) => {
            audit_secret(
                &state,
                &identity.ctx,
                "secret_store",
                AuditOutcome::Succeeded,
                None,
                &domain,
                &key,
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Err(source)) => {
            audit_secret(
                &state,
                &identity.ctx,
                "secret_store",
                AuditOutcome::Failed,
                Some(ApiErrorCode::ServiceUnavailable.as_str()),
                &domain,
                &key,
            );
            service_unavailable(format!("secret store failed: {source}"))
        }
        Err(source) => {
            audit_secret(
                &state,
                &identity.ctx,
                "secret_store",
                AuditOutcome::Failed,
                Some(ApiErrorCode::ServiceUnavailable.as_str()),
                &domain,
                &key,
            );
            service_unavailable(format!("secret store task failed: {source}"))
        }
    }
}
