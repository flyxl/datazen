//! §8 backend scope enforcement for migration Jobs.
//!
//! The desktop/browser surface only offers **same-backend** migration: a background
//! service can never reach a client-local profile, so any endpoint id that names
//! another backend must be rejected explicitly instead of being resolved lazily.
//!
//! Two layers, both fail closed:
//!
//! 1. Declaration — every migration request carries the backend scope of both
//!    endpoints. Only the local desktop backend is accepted; the declaration is
//!    mandatory so a remote/queued caller cannot omit the check.
//! 2. Endpoint reference shape — a local `dbSessionId` is an in-memory opaque token
//!    minted by this process. It never carries backend prefixes, path separators or
//!    authority syntax, so a reference that does cannot name a local session.

use serde::{Deserialize, Serialize};

use crate::commands::error::CommandError;
use crate::data_transfer::model::TransferJob;

/// The only backend scope a local migration Job may declare.
pub const LOCAL_BACKEND_SCOPE: &str = "local-desktop-backend";

/// Longest endpoint reference accepted as a local session token.
const MAX_LOCAL_SESSION_TOKEN: usize = 200;

/// Backend ownership declared by the caller for a migration request (§8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferBackendScope {
    /// Backend that owns the source endpoint.
    pub source_backend_scope: String,
    /// Backend that owns the target endpoint.
    pub target_backend_scope: String,
    /// Scopes of every profile the request touches. Desktop sessions all resolve
    /// to the local backend; a foreign entry is rejected instead of ignored.
    #[serde(default)]
    pub profile_backend_scopes: Vec<String>,
}

impl TransferBackendScope {
    /// The declaration a local desktop migration request sends.
    #[cfg(test)]
    pub fn local() -> Self {
        Self {
            source_backend_scope: LOCAL_BACKEND_SCOPE.to_string(),
            target_backend_scope: LOCAL_BACKEND_SCOPE.to_string(),
            profile_backend_scopes: vec![LOCAL_BACKEND_SCOPE.to_string()],
        }
    }
}

/// True when the reference has the shape of a locally minted session token.
pub fn is_local_session_reference(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_LOCAL_SESSION_TOKEN || id.trim() != id {
        return false;
    }
    !id.chars()
        .any(|ch| ch.is_whitespace() || "/\\@?#:".contains(ch))
}

/// §8 admission gate: reject anything that is not a same-backend migration.
pub fn enforce_same_backend_scope(
    job: &TransferJob,
    scope: Option<&TransferBackendScope>,
) -> Result<(), CommandError> {
    let Some(scope) = scope else {
        return Err(CommandError::Validation(
            "backendScope is required: migration endpoints must declare the local backend (§8)"
                .to_string(),
        ));
    };
    for (side, declared) in [
        ("source", &scope.source_backend_scope),
        ("target", &scope.target_backend_scope),
    ] {
        if declared.trim() != LOCAL_BACKEND_SCOPE {
            return Err(CommandError::Validation(format!(
                "{side} endpoint declares backend scope '{}': the desktop client only offers \
                 same-backend migration (§8)",
                redact(declared)
            )));
        }
    }
    if let Some(foreign) = scope
        .profile_backend_scopes
        .iter()
        .find(|declared| declared.trim() != LOCAL_BACKEND_SCOPE)
    {
        return Err(CommandError::Validation(format!(
            "request touches backend scope '{}': background services cannot reach a local profile (§8)",
            redact(foreign)
        )));
    }
    check_local_endpoint_reference("source", &job.source.db_session_id)?;
    // A SQL-file job has no database target (§6.1): it writes statements to a
    // local file, so there is no second session to resolve. Requiring one would
    // refuse every SQL-file migration before it starts.
    if job.sql_file_target.is_none() {
        let target = job.database_target().map_err(CommandError::from)?;
        check_local_endpoint_reference("target", &target.db_session_id)?;
    }
    Ok(())
}

fn check_local_endpoint_reference(side: &str, id: &str) -> Result<(), CommandError> {
    if is_local_session_reference(id) {
        return Ok(());
    }
    tracing::error!(
        side,
        "migration endpoint does not reference a local backend session"
    );
    Err(CommandError::Validation(format!(
        "{side} endpoint does not reference a session of the local backend; migration across \
         backends is not available (§8)"
    )))
}

/// Keep caller-declared backend names out of the response body in full.
fn redact(value: &str) -> String {
    const LIMIT: usize = 48;
    let trimmed = value.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(LIMIT).collect();
    format!("{head}…")
}
