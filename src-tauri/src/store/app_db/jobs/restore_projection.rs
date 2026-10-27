//! Restore admits only its typed frozen metadata, never scripts or local paths.
use datazen::restore_job::RestorePlan;
use datazen::shared::error::PortError;
use serde_json::Value;

use super::codec::safe_identifier;

pub(super) fn project(payload: &Value) -> Result<Value, PortError> {
    let invalid = || PortError::BackendUnavailable("restore durable plan is invalid".into());
    let plan: RestorePlan =
        serde_json::from_value(payload.get("restorePlan").cloned().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    // Re-encode from the typed whitelist. Equality rejects unknown fields at
    // every nesting level (ExecutionTarget itself tolerates unknown fields),
    // missing canonical fields, and envelope/plan identity or revision drift.
    let projected = plan.payload();
    if &projected != payload
        || !safe_identifier(&plan.plan_id)
        || !safe_identifier(plan.artifact_id.as_str())
        || !safe_identifier(plan.target.connection_id.as_str())
        || plan.input_bytes.get() == 0
        || plan.input_sha256.len() != 64
        || !plan.input_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    let namespace = &plan.target.namespace;
    let name = |s: &str| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control);
    if namespace.path.len() > 16
        || !namespace.path.iter().all(|s| name(s))
        || ![&namespace.database, &namespace.catalog, &namespace.schema]
            .into_iter()
            .flatten()
            .all(|s| name(s))
        || plan.target.object.as_ref().is_some_and(|o| {
            !safe_identifier(&o.kind)
                || !name(&o.name)
                || o.signature.as_ref().is_some_and(|s| !name(s))
        })
    {
        return Err(invalid());
    }
    Ok(projected)
}
