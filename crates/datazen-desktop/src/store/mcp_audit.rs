//! Append-only MCP tool audit log.
//!
//! T19 deliberately stores a JSONL log next to `datazen.sqlite` instead of
//! adding a schema migration: audit must never block the MCP call, and the log
//! is still durable and queryable. Rows contain only decision metadata — never
//! SQL, arguments, credentials, or result payloads.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{DesktopRepository, DesktopRepositoryError};

pub const MCP_AUDIT_FILE: &str = "mcp-audit.jsonl";
const MAX_LIST_LIMIT: usize = 500;

/// One audit row. Field names cross the IPC boundary in camelCase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuditRecord {
    pub id: String,
    pub organization_id: String,
    pub principal_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub namespace: Option<String>,
    pub tool: String,
    pub decision: String,
    pub row_count: Option<i64>,
    pub request_id: Option<String>,
    pub created_at: String,
}

/// Append-only file handle scoped to one app-data directory.
#[derive(Debug, Clone)]
pub struct McpAuditLog {
    path: PathBuf,
}

impl McpAuditLog {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(MCP_AUDIT_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one row. Callers log failures and continue; audit must not block
    /// the MCP call. Values outside the fixed decision vocabulary are rejected
    /// by the caller before this method, not silently stored.
    pub async fn append(&self, record: &McpAuditRecord) -> Result<(), DesktopRepositoryError> {
        if !matches!(
            record.decision.as_str(),
            "allow" | "deny" | "timeout" | "cancel" | "read"
        ) {
            return Err(DesktopRepositoryError::ParseError(format!(
                "invalid MCP audit decision: {}",
                record.decision
            )));
        }
        let mut line = serde_json::to_string(record)
            .map_err(|error| DesktopRepositoryError::ParseError(error.to_string()))?;
        line.push('\n');
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| DesktopRepositoryError::WriteError(error.to_string()))?;
        }
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|error| DesktopRepositoryError::WriteError(error.to_string()))?;
        file.write_all(line.as_bytes())
            .await
            .map_err(|error| DesktopRepositoryError::WriteError(error.to_string()))?;
        file.flush()
            .await
            .map_err(|error| DesktopRepositoryError::WriteError(error.to_string()))?;
        Ok(())
    }

    /// Newest-first rows, bounded by `limit`. Malformed historical lines are
    /// skipped so one bad append cannot make the audit UI unusable.
    pub async fn list(&self, limit: usize) -> Result<Vec<McpAuditRecord>, DesktopRepositoryError> {
        let limit = limit.clamp(1, MAX_LIST_LIMIT);
        let body = match tokio::fs::read_to_string(&self.path).await {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(DesktopRepositoryError::WriteError(error.to_string())),
        };
        let mut rows: Vec<McpAuditRecord> = body
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        rows.truncate(limit);
        Ok(rows)
    }
}

impl DesktopRepository {
    pub fn mcp_audit_log(&self) -> McpAuditLog {
        McpAuditLog::new(&self.data_dir)
    }
}

pub fn new_audit_record(
    organization_id: impl Into<String>,
    principal_id: impl Into<String>,
    connection_id: Option<String>,
    database_name: Option<String>,
    namespace: Option<String>,
    tool: impl Into<String>,
    decision: impl Into<String>,
    row_count: Option<i64>,
    request_id: Option<String>,
) -> McpAuditRecord {
    McpAuditRecord {
        id: format!("mcp-audit-{}", uuid::Uuid::new_v4()),
        organization_id: organization_id.into(),
        principal_id: principal_id.into(),
        connection_id,
        database_name,
        namespace,
        tool: tool.into(),
        decision: decision.into(),
        row_count,
        request_id,
        created_at: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audit_is_persisted_newest_first_and_never_contains_payloads() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = McpAuditLog::new(temp.path());
        let mut first = new_audit_record(
            "org",
            "who",
            Some("c1".into()),
            Some("db".into()),
            Some("public".into()),
            "query",
            "allow",
            Some(7),
            Some("r1".into()),
        );
        first.created_at = "2026-01-01T00:00:00Z".into();
        let mut second = new_audit_record(
            "org",
            "who",
            Some("c1".into()),
            None,
            None,
            "run_workflow",
            "deny",
            None,
            Some("r2".into()),
        );
        second.created_at = "2026-01-02T00:00:00Z".into();
        log.append(&first).await.unwrap();
        log.append(&second).await.unwrap();

        let rows = log.list(10).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].request_id.as_deref(), Some("r2"));
        assert_eq!(rows[0].decision, "deny");
        assert_eq!(rows[1].row_count, Some(7));
        let raw = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert!(!raw.contains("password"));
        assert!(!raw.contains("SELECT"));
    }

    #[tokio::test]
    async fn missing_log_is_empty_and_invalid_decisions_are_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = McpAuditLog::new(temp.path());
        assert!(log.list(10).await.unwrap().is_empty());
        let mut record =
            new_audit_record("org", "who", None, None, None, "query", "maybe", None, None);
        assert!(log.append(&record).await.is_err());
        record.decision = "timeout".into();
        assert!(log.append(&record).await.is_ok());
    }
}
