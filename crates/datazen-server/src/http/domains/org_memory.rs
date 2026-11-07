//! Organization-memory domain (T25).
//!
//! Organization memories are server-owned because the organization role model
//! (`Role::OrgAdmin`) lives here, not in the local desktop profile. Members can
//! list and report injection decisions; only an organization administrator can
//! publish a version or revoke one. The organization is always the one stamped
//! on the authenticated session (INV-01); no request body can select a tenant.
//!
//! Wire:
//!
//! | method | route | caller | success |
//! | --- | --- | --- | --- |
//! | GET | `/api/v1/org-memory` | member | `{ entries: OrgMemoryEntry[] }` |
//! | POST | `/api/v1/org-memory` | admin | `OrgMemoryEntry` |
//! | DELETE | `/api/v1/org-memory/{id}` | admin | 204 |
//! | GET | `/api/v1/org-memory/audit` | admin | `{ entries: OrgMemoryAuditEntry[] }` |
//! | POST | `/api/v1/org-memory/{id}/injections` | member | 204 |
//!
//! Audit rows carry who/organization/memory/version/decision and never the
//! memory subject or content.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use datazen::ai::memory::{
    OrgMemoryAuditEntry, OrgMemoryDecision, OrgMemoryDraft, OrgMemoryEntry, OrgMemoryError,
};
use datazen::shared::error::{ApiError, ApiErrorCode};

use crate::http::middleware::authorize::NOT_VISIBLE_MESSAGE;
use crate::http::response::ApiErrorResponse;
use crate::http::router::{api_error_response, invalid_argument, SessionIdentity};
use crate::policy::PolicyService;
use crate::storage::{AdminStore, ServerOrgMemories};

#[derive(Clone)]
pub struct OrgMemoryState {
    pub store: Arc<AdminStore>,
    pub policy: Arc<PolicyService>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrgMemoryList {
    entries: Vec<OrgMemoryEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrgMemoryAuditList {
    entries: Vec<OrgMemoryAuditEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InjectionAuditRequest {
    version: u32,
    decision: OrgMemoryDecision,
}

pub fn org_memory_router(state: OrgMemoryState) -> Router {
    Router::new()
        .route(
            "/api/v1/org-memory",
            get(handler_list).post(handler_publish),
        )
        .route("/api/v1/org-memory/audit", get(handler_audit))
        .route("/api/v1/org-memory/{id}", delete(handler_revoke))
        .route(
            "/api/v1/org-memory/{id}/injections",
            post(handler_record_injection),
        )
        .with_state(state)
}

fn memories(state: &OrgMemoryState, organization_id: &str) -> ServerOrgMemories {
    ServerOrgMemories::scoped(organization_id, state.store.clone())
}

fn ensure_admin(state: &OrgMemoryState, identity: &SessionIdentity) -> Result<(), Response> {
    match state.policy.is_org_admin(
        identity.ctx.organization_id.as_str(),
        identity.ctx.principal_id.as_str(),
    ) {
        Ok(true) => Ok(()),
        Ok(false) => Err(ApiErrorResponse::new(ApiError::new(
            ApiErrorCode::PermissionDenied,
            "Only an organization administrator can publish or revoke organization memories.",
        ))
        .with_status(StatusCode::FORBIDDEN)
        .into_response()),
        Err(source) => Err(service_unavailable(format!(
            "organization role lookup failed: {source}"
        ))),
    }
}

fn service_unavailable(message: impl Into<String>) -> Response {
    api_error_response(ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        message.into(),
    ))
}

fn map_error(error: OrgMemoryError) -> Response {
    match error {
        OrgMemoryError::Validation(message) => invalid_argument(message),
        OrgMemoryError::NotFound => {
            api_error_response(ApiError::new(ApiErrorCode::NotFound, NOT_VISIBLE_MESSAGE))
        }
        OrgMemoryError::Repository(message) => service_unavailable(message),
    }
}

async fn handler_list(State(state): State<OrgMemoryState>, identity: SessionIdentity) -> Response {
    let service = memories(&state, identity.ctx.organization_id.as_str());
    match service.list_active() {
        Ok(entries) => (StatusCode::OK, Json(OrgMemoryList { entries })).into_response(),
        Err(error) => map_error(error),
    }
}

async fn handler_publish(
    State(state): State<OrgMemoryState>,
    identity: SessionIdentity,
    Json(draft): Json<OrgMemoryDraft>,
) -> Response {
    if let Err(response) = ensure_admin(&state, &identity) {
        return response;
    }
    let service = memories(&state, identity.ctx.organization_id.as_str());
    match service.publish(
        &draft,
        identity.ctx.principal_id.as_str(),
        chrono::Utc::now(),
    ) {
        Ok(entry) => (StatusCode::CREATED, Json(entry)).into_response(),
        Err(error) => map_error(error),
    }
}

async fn handler_revoke(
    State(state): State<OrgMemoryState>,
    identity: SessionIdentity,
    Path(id): Path<String>,
) -> Response {
    if let Err(response) = ensure_admin(&state, &identity) {
        return response;
    }
    let service = memories(&state, identity.ctx.organization_id.as_str());
    match service.revoke(&id, identity.ctx.principal_id.as_str(), chrono::Utc::now()) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => map_error(error),
    }
}

async fn handler_audit(State(state): State<OrgMemoryState>, identity: SessionIdentity) -> Response {
    if let Err(response) = ensure_admin(&state, &identity) {
        return response;
    }
    let service = memories(&state, identity.ctx.organization_id.as_str());
    match service.list_audit() {
        Ok(entries) => (StatusCode::OK, Json(OrgMemoryAuditList { entries })).into_response(),
        Err(error) => map_error(error),
    }
}

async fn handler_record_injection(
    State(state): State<OrgMemoryState>,
    identity: SessionIdentity,
    Path(id): Path<String>,
    Json(body): Json<InjectionAuditRequest>,
) -> Response {
    let service = memories(&state, identity.ctx.organization_id.as_str());
    match service.record_injection(
        &id,
        body.version,
        identity.ctx.principal_id.as_str(),
        body.decision,
        chrono::Utc::now(),
    ) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => map_error(error),
    }
}
