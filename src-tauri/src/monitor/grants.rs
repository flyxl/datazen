//! Local schedule grants are issued by a desktop user and held as backend metadata.
use crate::{commands::AppState, store::DesktopRepository};
use datazen_feature_core::{
    dashboard::schedule_grant::{MonitorGrant, MonitorScope},
    workflow::{
        context::WorkflowContext,
        model::{WorkflowDefinition, WorkflowStep},
    },
};
use datazen_platform_api::{
    context::RequestContext,
    id::{ConnectionId, Counter},
    target::{ExecutionTarget, NamespaceTarget},
};
use sha2::{Digest, Sha256};
pub(crate) fn revision(yaml: &str) -> String {
    format!("{:x}", Sha256::digest(yaml.as_bytes()))
}

pub(crate) fn current(
    store: &DesktopRepository,
    context: &RequestContext,
) -> Result<MonitorGrant, String> {
    let id = context
        .delegation_id
        .as_ref()
        .ok_or("monitor delegation required")?;
    let db = store.app_db();
    let grant = db
        .monitor_grant(id.as_str())
        .map_err(|_| "monitor grant lookup failed")?
        .ok_or("monitor grant missing")?;
    grant
        .validate(context, chrono::Utc::now())
        .map_err(str::to_string)?;
    let latest = db
        .widget_monitor_grant(&grant.dashboard_id, &grant.widget_id)
        .map_err(|_| "monitor grant lookup failed")?
        .ok_or("monitor grant missing")?;
    if latest != grant {
        return Err("monitor grant superseded".into());
    }
    let dashboard = db
        .get_dashboard(&grant.dashboard_id)
        .map_err(|_| "monitor dashboard missing")?;
    let widget = db
        .get_widget(&grant.widget_id)
        .map_err(|_| "monitor widget missing")?;
    if !dashboard.enabled
        || dashboard.refresh_paused
        || !widget.enabled
        || widget.dashboard_id != grant.dashboard_id
        || widget.workflow_id != grant.workflow_id
        || widget.refresh_mode != "interval"
    {
        return Err("monitor schedule disabled or changed".into());
    }
    let workflow = db
        .get_workflow(&grant.workflow_id)
        .map_err(|_| "monitor workflow missing")?;
    if revision(&workflow.definition_yaml) != grant.workflow_revision {
        return Err("monitor workflow revision changed".into());
    }
    Ok(grant)
}

pub(crate) async fn authorize(
    state: &AppState,
    dashboard_id: &str,
    widget_id: &str,
    ttl_secs: u32,
) -> Result<MonitorGrant, String> {
    if !(60..=86400).contains(&ttl_secs) {
        return Err("monitor grant lifetime must be 60..86400 seconds".into());
    }
    let adapter = state
        .platform_require()
        .map_err(|_| "platform unavailable")?;
    let call = adapter.begin_call();
    let user = call.context();
    if user.is_delegated() || Some(user.principal_id.clone()) != crate::platform::local_principal()
    {
        return Err("desktop user authorization required".into());
    }
    let db = state.store.app_db();
    let dashboard = db
        .get_dashboard(dashboard_id)
        .map_err(|_| "dashboard missing")?;
    let widget = db.get_widget(widget_id).map_err(|_| "widget missing")?;
    if !dashboard.enabled
        || dashboard.refresh_paused
        || !widget.enabled
        || widget.dashboard_id != dashboard_id
        || widget.refresh_mode != "interval"
    {
        return Err("enable the interval schedule before authorizing it".into());
    }
    let record = db
        .get_workflow(&widget.workflow_id)
        .map_err(|_| "workflow missing")?;
    let workflow: WorkflowDefinition =
        serde_yaml::from_str(&record.definition_yaml).map_err(|_| "workflow invalid")?;
    if !workflow.variables.is_empty() || !workflow.blocks.is_empty() {
        return Err("scheduled monitor supports fixed read-only query steps only".into());
    }
    workflow
        .clone()
        .into_executable()
        .map_err(|_| "workflow version invalid")?;
    let normalized = super::fixed_targets::prepare_definition(
        &workflow,
        &WorkflowContext::new(&serde_json::json!({})),
    )
    .map_err(|_| "explicit workflow targets required")?;
    let mut scopes = Vec::new();
    for step in &normalized.steps {
        let (command, sql, connection, database, schema) = match step {
            WorkflowStep::Query {
                sql,
                connection,
                database,
                ..
            } => (
                "query",
                sql.as_str(),
                connection.as_deref(),
                database.as_deref(),
                None,
            ),
            WorkflowStep::Command {
                command,
                connection,
                input,
                ..
            } if command == "query" => (
                command.as_str(),
                input["sql"].as_str().ok_or("SQL required")?,
                connection.as_deref(),
                input["database"].as_str(),
                input["schema"].as_str(),
            ),
            _ => return Err("scheduled monitor supports fixed read-only query steps only".into()),
        };
        if sql.contains("{{") || sql.contains("${") {
            return Err("scheduled SQL must not contain dynamic templates".into());
        }
        datazen_feature_core::dashboard::schedule_grant::validate_read_only_sql(sql)
            .map_err(|_| "scheduled monitor requires read-only SQL")?;
        let connection = connection.ok_or("explicit connection required")?;
        let database = database
            .filter(|v| !v.trim().is_empty())
            .ok_or("explicit database required")?;
        let (config, metadata) = state
            .store
            .platform_profile(connection)
            .await
            .ok_or("connection missing")?;
        if !metadata.enabled || !matches!(config.database_type.as_str(), "postgresql" | "mysql") {
            return Err("scheduled monitor requires enabled PostgreSQL/MySQL profile".into());
        }
        let target = ExecutionTarget::new(
            ConnectionId::new(connection),
            NamespaceTarget::new(
                Some(database.into()),
                None,
                schema.map(str::to_owned).or(config.schema),
                vec![],
            ),
        );
        let canonical = adapter
            .resolve_execution_target(
                user,
                Counter::new(metadata.config_revision),
                command,
                &target,
            )
            .await
            .map_err(|_| "monitor target invalid")?;
        scopes.push(MonitorScope {
            target: crate::platform::into_execution_target(canonical),
            config_revision: metadata.config_revision,
            command: command.into(),
        });
    }
    let id = uuid::Uuid::new_v4().to_string();
    let grant = MonitorGrant {
        id: id.clone(),
        dashboard_id: dashboard_id.into(),
        widget_id: widget_id.into(),
        workflow_id: widget.workflow_id,
        workflow_revision: revision(&record.definition_yaml),
        organization_id: user.organization_id.as_str().into(),
        service_principal: format!("monitor-service:{id}"),
        approved_by: user.principal_id.as_str().into(),
        scopes,
        actions: vec!["connect".into(), "query".into()],
        read_only: true,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(i64::from(ttl_secs)),
        revocation_version: 1,
        revoked: false,
    };
    // Re-read frozen facts after async metadata/driver checks before saving approval.
    if revision(
        &db.get_workflow(&grant.workflow_id)
            .map_err(|_| "workflow missing")?
            .definition_yaml,
    ) != grant.workflow_revision
    {
        return Err("workflow changed during authorization".into());
    }
    db.save_monitor_grant(&grant)
        .map_err(|_| "monitor grant persistence failed")?;
    current(&state.store, &grant.context())?;
    Ok(grant)
}
pub(crate) fn revoke(state: &AppState, dashboard_id: &str, widget_id: &str) -> Result<(), String> {
    let call = state
        .platform_require()
        .map_err(|_| "platform unavailable")?
        .begin_call();
    let db = state.store.app_db();
    if let Some(mut grant) = db
        .widget_monitor_grant(dashboard_id, widget_id)
        .map_err(|_| "grant lookup failed")?
    {
        if grant.approved_by != call.context().principal_id.as_str()
            || call.context().is_delegated()
        {
            return Err("grant owner required".into());
        }
        grant.revoked = true;
        grant.revocation_version = grant
            .revocation_version
            .checked_add(1)
            .ok_or("grant version overflow")?;
        db.save_monitor_grant(&grant)
            .map_err(|_| "grant revoke failed")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "grants_tests.rs"]
mod tests;
