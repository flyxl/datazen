//! Stable identity rules for scheduled monitor occurrences.

use crate::monitor::domain::MonitorGrant;
use crate::shared::id::IdempotencyKey;
use sha2::{Digest, Sha256};

pub fn schedule_id(dashboard_id: &str, widget_id: &str) -> String {
    format!("monitor:{dashboard_id}:{widget_id}")
}

pub fn definition_revision(
    grant: &MonitorGrant,
    refresh_interval_seconds: u64,
) -> Result<String, String> {
    let mut scopes = grant.scopes.clone();
    scopes.sort_by(|left, right| {
        left.target
            .connection_id
            .cmp(&right.target.connection_id)
            .then_with(|| {
                left.target
                    .namespace
                    .namespace_fingerprint()
                    .cmp(&right.target.namespace.namespace_fingerprint())
            })
            .then_with(|| left.command.cmp(&right.command))
            .then_with(|| left.config_revision.cmp(&right.config_revision))
    });
    let identity = serde_json::json!({
        "dashboardId": grant.dashboard_id,
        "widgetId": grant.widget_id,
        "workflowId": grant.workflow_id,
        "workflowRevision": grant.workflow_revision,
        "grantId": grant.id,
        "grantVersion": grant.revocation_version,
        "refreshIntervalSeconds": refresh_interval_seconds,
        "scopes": scopes,
    });
    let identity =
        serde_json::to_vec(&identity).map_err(|_| "scheduled definition revision unavailable")?;
    Ok(format!("sha256:{:x}", Sha256::digest(identity)))
}

pub fn idempotency_key(schedule: &str, definition: &str, scheduled_at: i64) -> IdempotencyKey {
    let identity = format!(
        "{}:{}{}:{}{}",
        schedule.len(),
        schedule,
        definition.len(),
        definition,
        scheduled_at
    );
    IdempotencyKey::new(format!(
        "monitor-occurrence:{:x}",
        Sha256::digest(identity.as_bytes())
    ))
}
