//! Append-only audit for reviewed write proposals (T24).
//!
//! Rows contain the target, exact-SQL SHA-256 digest, decision and affected-row
//! count only. SQL text, bound parameters and connection credentials never
//! enter this log.

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{DesktopRepository, DesktopRepositoryError};

pub const AI_WRITE_AUDIT_FILE: &str = "ai-write-audit.jsonl";
const MAX_LIST_LIMIT: usize = 500;

/// One proposal lifecycle row. The decision vocabulary is deliberately
/// three-valued: a proposal was shown, applied, or denied/refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWriteAuditRecord {
    pub id: String,
    pub organization_id: String,
    pub principal_id: String,
    pub proposal_id: Option<String>,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub sql_hash: String,
    /// `proposal`, `applied`, or `denied`.
    pub decision: String,
    pub row_count: Option<u64>,
    /// Stable short reason (guard/expiry/mismatch/user_denied). Never SQL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: String,
}

/// Append-only file handle scoped to one app-data directory.
#[derive(Debug, Clone)]
pub struct AiWriteAuditLog {
    path: PathBuf,
}

impl AiWriteAuditLog {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(AI_WRITE_AUDIT_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn append(&self, record: &AiWriteAuditRecord) -> Result<(), DesktopRepositoryError> {
        if !matches!(record.decision.as_str(), "proposal" | "applied" | "denied") {
            return Err(DesktopRepositoryError::ParseError(format!(
                "invalid AI write audit decision: {}",
                record.decision
            )));
        }
        if record.sql_hash.is_empty() {
            return Err(DesktopRepositoryError::ParseError(
                "AI write audit requires a SQL hash".into(),
            ));
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

    pub async fn list(
        &self,
        limit: usize,
    ) -> Result<Vec<AiWriteAuditRecord>, DesktopRepositoryError> {
        let limit = limit.clamp(1, MAX_LIST_LIMIT);
        let body = match tokio::fs::read_to_string(&self.path).await {
            Ok(body) => body,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(DesktopRepositoryError::WriteError(error.to_string())),
        };
        let mut rows: Vec<AiWriteAuditRecord> = body
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        rows.truncate(limit);
        Ok(rows)
    }
}

impl DesktopRepository {
    pub fn ai_write_audit_log(&self) -> AiWriteAuditLog {
        AiWriteAuditLog::new(&self.data_dir)
    }
}

#[derive(Debug, Clone)]
pub struct NewAiWriteAuditRecord {
    pub organization_id: String,
    pub principal_id: String,
    pub proposal_id: Option<String>,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub sql_hash: String,
    pub decision: String,
    pub row_count: Option<u64>,
    pub reason: Option<String>,
}

pub fn new_ai_write_audit_record(input: NewAiWriteAuditRecord) -> AiWriteAuditRecord {
    AiWriteAuditRecord {
        id: format!("ai-write-audit-{}", uuid::Uuid::new_v4()),
        organization_id: input.organization_id,
        principal_id: input.principal_id,
        proposal_id: input.proposal_id,
        connection_id: input.connection_id,
        database_name: input.database_name,
        sql_hash: input.sql_hash,
        decision: input.decision,
        row_count: input.row_count,
        reason: input.reason,
        created_at: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(decision: &str) -> AiWriteAuditRecord {
        new_ai_write_audit_record(NewAiWriteAuditRecord {
            organization_id: "org".into(),
            principal_id: "who".into(),
            proposal_id: Some("p1".into()),
            connection_id: Some("c1".into()),
            database_name: Some("db".into()),
            sql_hash: "a".repeat(64),
            decision: decision.into(),
            row_count: None,
            reason: None,
        })
    }

    #[tokio::test]
    async fn persists_proposal_applied_and_denied_without_sql() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = AiWriteAuditLog::new(temp.path());
        let mut proposed = record("proposal");
        proposed.created_at = "2026-01-01T00:00:00Z".into();
        let mut applied = record("applied");
        applied.row_count = Some(3);
        applied.created_at = "2026-01-01T00:01:00Z".into();
        let mut denied = record("denied");
        denied.reason = Some("user_denied".into());
        denied.created_at = "2026-01-01T00:02:00Z".into();
        for row in [&proposed, &applied, &denied] {
            log.append(row).await.unwrap();
        }
        let rows = log.list(10).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].decision, "denied");
        assert_eq!(rows[0].reason.as_deref(), Some("user_denied"));
        assert_eq!(rows[1].row_count, Some(3));
        assert_eq!(rows[2].decision, "proposal");
        let raw = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert!(!raw.contains("DELETE FROM"));
        assert!(!raw.contains("password"));
        assert!(raw.contains(&"a".repeat(64)));
    }

    #[tokio::test]
    async fn rejects_unknown_decisions_and_missing_hash() {
        let temp = tempfile::TempDir::new().unwrap();
        let log = AiWriteAuditLog::new(temp.path());
        assert!(log.append(&record("maybe")).await.is_err());
        let mut missing = record("proposal");
        missing.sql_hash.clear();
        assert!(log.append(&missing).await.is_err());
    }
}
