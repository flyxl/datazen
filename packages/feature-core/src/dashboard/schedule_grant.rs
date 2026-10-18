//! Backend-held, revocable metadata for one Monitor widget. No bearer credentials.
use chrono::{DateTime, Utc};
use datazen_platform_api::{
    context::{DelegationRef, RequestContext},
    id::*,
    target::ExecutionTarget,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorScope {
    pub target: ExecutionTarget,
    pub config_revision: u64,
    pub command: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorGrant {
    pub id: String,
    pub dashboard_id: String,
    pub widget_id: String,
    pub workflow_id: String,
    pub workflow_revision: String,
    pub organization_id: String,
    pub service_principal: String,
    pub approved_by: String,
    pub scopes: Vec<MonitorScope>,
    pub actions: Vec<String>,
    pub read_only: bool,
    pub expires_at: DateTime<Utc>,
    pub revocation_version: u64,
    pub revoked: bool,
}
impl MonitorGrant {
    pub fn validate(
        &self,
        context: &RequestContext,
        now: DateTime<Utc>,
    ) -> Result<(), &'static str> {
        if self.revoked
            || self.expires_at <= now
            || self.revocation_version == 0
            || !self.read_only
            || self.scopes.is_empty()
            || self.actions.len() != 2
            || !self.actions.iter().any(|action| action == "connect")
            || !self.actions.iter().any(|action| action == "query")
        {
            return Err("monitor grant revoked, expired or invalid");
        }
        if context.authentication_session_id.is_some()
            || context.organization_id.as_str() != self.organization_id
            || context.principal_id.as_str() != self.service_principal
            || context.client_instance_id.as_str()
                != format!("monitor:{}:{}", self.id, self.revocation_version)
            || context.delegation_id.as_ref().map(DelegationId::as_str) != Some(self.id.as_str())
        {
            return Err("monitor service identity mismatch");
        }
        Ok(())
    }
    pub fn context(&self) -> RequestContext {
        RequestContext::new(
            OrganizationId::new(&self.organization_id),
            PrincipalId::new(&self.service_principal),
            None,
            ClientInstanceId::new(format!("monitor:{}:{}", self.id, self.revocation_version)),
            RequestId::new(uuid::Uuid::new_v4().to_string()),
            Some(DelegationId::new(&self.id)),
        )
    }
    pub fn delegation(&self) -> DelegationRef {
        let context = self.context();
        DelegationRef::new(
            DelegationId::new(&self.id),
            context.organization_id,
            context.principal_id,
            context.client_instance_id,
            format!(
                "monitor:{}:{}:{}:{}",
                self.dashboard_id, self.widget_id, self.workflow_revision, self.revocation_version
            ),
            Timestamp::new(self.expires_at.to_rfc3339()),
        )
    }
}

/// Bounded monitor policy: one SELECT/VALUES query, no writable CTE, INTO or row
/// lock. Function calls use a closed built-in allowlist; unknown, quoted and
/// schema-qualified functions and table-valued functions fail closed. Relations
/// must be single identifiers: SQL cannot override the approved target namespace
/// through schema/database qualification. This supplements the connection SQL
/// guard; it is not MCP's ReadOnly
/// mode (which deliberately disables all SQL tools).
pub fn validate_read_only_sql(sql: &str) -> Result<(), String> {
    use sqlparser::{
        ast::{Expr, ObjectNamePart, Query, SetExpr, Statement, TableFactor, Visit, Visitor},
        dialect::GenericDialect,
        parser::Parser,
    };
    use std::ops::ControlFlow;
    datazen_runtime::sql_guard::reject_null_bytes(sql)?;
    datazen_runtime::sql_guard::check_sql(sql, true, false)?;
    fn body(query: &SetExpr) -> bool {
        match query {
            SetExpr::Select(select) => select.into.is_none(),
            SetExpr::Query(query) => query.locks.is_empty() && body(&query.body),
            SetExpr::SetOperation { left, right, .. } => body(left) && body(right),
            SetExpr::Values(_) => true,
            _ => false,
        }
    }
    struct ReadOnly;
    impl Visitor for ReadOnly {
        type Break = ();
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
            if let Expr::Function(function) = expr {
                // Do not infer safety from SELECT or maintain a denylist of
                // volatile functions: new/UDF/qualified calls must be reviewed.
                let [ObjectNamePart::Identifier(name)] = function.name.0.as_slice() else {
                    return ControlFlow::Break(());
                };
                if name.quote_style.is_some()
                    || !matches!(
                        name.value.to_ascii_lowercase().as_str(),
                        "count"
                            | "sum"
                            | "avg"
                            | "min"
                            | "max"
                            | "abs"
                            | "round"
                            | "ceil"
                            | "ceiling"
                            | "floor"
                            | "sqrt"
                            | "lower"
                            | "upper"
                            | "length"
                            | "char_length"
                            | "octet_length"
                            | "trim"
                            | "ltrim"
                            | "rtrim"
                            | "replace"
                            | "substring"
                            | "substr"
                            | "concat"
                            | "coalesce"
                            | "nullif"
                            | "greatest"
                            | "least"
                            | "now"
                            | "current_date"
                            | "current_timestamp"
                    )
                {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<()> {
            // FROM f(...) is not necessarily represented by Expr::Function.
            match table {
                // The Runtime target grant does not constrain qualified names
                // embedded in SQL. Without a dialect-aware namespace comparison,
                // even a qualification that looks like the approved namespace
                // must fail closed (including quoted and multipart variants).
                TableFactor::Table {
                    name, args: None, ..
                } if matches!(name.0.as_slice(), [ObjectNamePart::Identifier(_)]) => {
                    ControlFlow::Continue(())
                }
                TableFactor::Derived { .. } | TableFactor::NestedJoin { .. } => {
                    ControlFlow::Continue(())
                }
                _ => ControlFlow::Break(()),
            }
        }
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
            if query.locks.is_empty() && body(&query.body) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        }
    }
    let statements = Parser::parse_sql(&GenericDialect {}, sql)
        .map_err(|_| "monitor SQL cannot be verified as a read-only query")?;
    if statements.len() != 1
        || !matches!(statements.first(), Some(Statement::Query(_)))
        || statements.visit(&mut ReadOnly).is_break()
    {
        return Err(
            "monitor requires one read-only query without writable CTE, INTO, row locks, qualified relations or unapproved functions".into(),
        );
    }
    Ok(())
}
