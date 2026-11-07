//! Server-side organization-memory persistence (T25).
//!
//! This is deliberately separate from the desktop `ai_memories` table: an
//! organization memory is shared, versioned, and administrator-approved. The
//! adapter is scoped per request organization; every query includes
//! `organization_id`, so a memory from another tenant cannot be read, revoked,
//! or audited through this instance.

use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use datazen::ai::memory::{
    latest_active_org_memories, next_org_memory_version, validate_org_memory_draft, MemoryKind,
    MemorySensitivity, OrgMemoryAuditAction, OrgMemoryAuditEntry, OrgMemoryDecision,
    OrgMemoryDraft, OrgMemoryEntry, OrgMemoryError, OrgMemoryStatus,
};

use crate::storage::AdminStore;

fn backend(error: impl std::fmt::Display) -> OrgMemoryError {
    OrgMemoryError::Repository(error.to_string())
}

fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, OrgMemoryError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| {
            OrgMemoryError::Repository("stored organization memory has an invalid timestamp".into())
        })
}

/// A per-organization view of the admin store.
#[derive(Clone)]
pub struct ServerOrgMemories {
    store: Arc<AdminStore>,
    organization_id: String,
}

impl ServerOrgMemories {
    pub fn scoped(organization_id: &str, store: Arc<AdminStore>) -> Self {
        Self {
            store,
            organization_id: organization_id.to_string(),
        }
    }

    pub fn organization_id(&self) -> &str {
        &self.organization_id
    }

    /// Active newest versions only. Revoked rows are filtered in SQL and the
    /// shared latest-version helper closes the "older active version" hole.
    pub fn list_active(&self) -> Result<Vec<OrgMemoryEntry>, OrgMemoryError> {
        let conn = self.store.lock_for_adapter().map_err(backend)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, version, kind, subject, content, confidence, sensitivity, status, \
                        created_by, approved_by, published_at, revoked_at, revoked_by \
                   FROM org_memories \
                  WHERE organization_id = ?1 AND status = 'active' \
                  ORDER BY published_at DESC, id ASC",
            )
            .map_err(backend)?;
        let organization_id = self.organization_id.clone();
        let rows = stmt
            .query_map([self.organization_id.as_str()], |row| {
                row_to_entry(row, &organization_id)
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(latest_active_org_memories(&rows))
    }

    /// Publish a new version. `actor` is the authenticated administrator; the
    /// route checks the role before calling this adapter, and the row records
    /// both `created_by` and `approved_by` for audit.
    pub fn publish(
        &self,
        draft: &OrgMemoryDraft,
        actor: &str,
        now: DateTime<Utc>,
    ) -> Result<OrgMemoryEntry, OrgMemoryError> {
        validate_org_memory_draft(draft)?;
        let id = draft
            .memory_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let when = timestamp(now);
        let mut conn = self.store.lock_for_adapter().map_err(backend)?;
        let tx = conn.transaction().map_err(backend)?;
        let versions: Vec<u32> = {
            let mut stmt = tx
                .prepare("SELECT version FROM org_memories WHERE organization_id=?1 AND id=?2")
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![self.organization_id, id], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(backend)?
                .map(|row| row.map(|version| version.max(0) as u32))
                .collect::<Result<Vec<_>, _>>()
                .map_err(backend)?;
            rows
        };
        let version = next_org_memory_version(versions);
        // Publishing a new version supersedes every older active version.
        // Marking them revoked is what makes a late client holding the old
        // version record a `suppressed` injection instead of a stale hit.
        tx.execute(
            "UPDATE org_memories SET status='revoked', revoked_at=?1, revoked_by=?2 \
              WHERE organization_id=?3 AND id=?4 AND status='active'",
            params![when, actor, self.organization_id, id],
        )
        .map_err(backend)?;
        let entry = OrgMemoryEntry {
            id: id.clone(),
            organization_id: self.organization_id.clone(),
            version,
            kind: draft.kind,
            subject: draft.subject.trim().to_string(),
            content: draft.content.trim().to_string(),
            confidence: draft.confidence,
            sensitivity: draft.sensitivity,
            status: OrgMemoryStatus::Active,
            created_by: actor.to_string(),
            approved_by: actor.to_string(),
            published_at: now,
            revoked_at: None,
            revoked_by: None,
        };
        tx.execute(
            "INSERT INTO org_memories (\
               organization_id, id, version, kind, subject, content, confidence, sensitivity, \
               status, created_by, approved_by, published_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?9, ?10)",
            params![
                self.organization_id,
                entry.id,
                i64::from(entry.version),
                entry.kind.as_str(),
                entry.subject,
                entry.content,
                entry.confidence,
                entry.sensitivity.as_str(),
                actor,
                when,
            ],
        )
        .map_err(backend)?;
        write_audit(
            &tx,
            &self.organization_id,
            &entry.id,
            entry.version,
            OrgMemoryAuditAction::Publish,
            actor,
            OrgMemoryDecision::Approved,
            None,
            &when,
        )?;
        tx.commit().map_err(backend)?;
        Ok(entry)
    }

    /// Revoke every active version of one memory id in this organization.
    /// The update is immediate: the next list/retrieval query cannot see it.
    pub fn revoke(
        &self,
        memory_id: &str,
        actor: &str,
        now: DateTime<Utc>,
    ) -> Result<u32, OrgMemoryError> {
        let when = timestamp(now);
        let mut conn = self.store.lock_for_adapter().map_err(backend)?;
        let tx = conn.transaction().map_err(backend)?;
        let version = tx
            .query_row(
                "SELECT MAX(version) FROM org_memories \
                  WHERE organization_id=?1 AND id=?2 AND status='active'",
                params![self.organization_id, memory_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(backend)?
            .ok_or(OrgMemoryError::NotFound)?;
        let changed = tx
            .execute(
                "UPDATE org_memories SET status='revoked', revoked_at=?1, revoked_by=?2 \
                  WHERE organization_id=?3 AND id=?4 AND status='active'",
                params![when, actor, self.organization_id, memory_id],
            )
            .map_err(backend)?;
        if changed == 0 {
            return Err(OrgMemoryError::NotFound);
        }
        write_audit(
            &tx,
            &self.organization_id,
            memory_id,
            version.max(0) as u32,
            OrgMemoryAuditAction::Revoke,
            actor,
            OrgMemoryDecision::Revoked,
            None,
            &when,
        )?;
        tx.commit().map_err(backend)?;
        Ok(changed as u32)
    }

    /// Record a client-side injection decision. `injected` is downgraded to
    /// `suppressed` when the addressed version is already revoked, so the
    /// audit can never claim a revoked fact reached a prompt.
    pub fn record_injection(
        &self,
        memory_id: &str,
        version: u32,
        actor: &str,
        decision: OrgMemoryDecision,
        now: DateTime<Utc>,
    ) -> Result<(), OrgMemoryError> {
        if !matches!(
            decision,
            OrgMemoryDecision::Injected | OrgMemoryDecision::Suppressed
        ) {
            return Err(OrgMemoryError::Validation(
                "injection audit decision must be injected or suppressed".into(),
            ));
        }
        let when = timestamp(now);
        let conn = self.store.lock_for_adapter().map_err(backend)?;
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM org_memories \
                  WHERE organization_id=?1 AND id=?2 AND version=?3",
                params![self.organization_id, memory_id, i64::from(version)],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let status = status.ok_or(OrgMemoryError::NotFound)?;
        let decision = if status == "active" {
            decision
        } else {
            OrgMemoryDecision::Suppressed
        };
        write_audit(
            &conn,
            &self.organization_id,
            memory_id,
            version,
            OrgMemoryAuditAction::Inject,
            actor,
            decision,
            None,
            &when,
        )
    }

    pub fn list_audit(&self) -> Result<Vec<OrgMemoryAuditEntry>, OrgMemoryError> {
        let conn = self.store.lock_for_adapter().map_err(backend)?;
        let mut stmt = conn
            .prepare(
                "SELECT id, memory_id, version, action, actor_principal_id, decision, \
                        detail_json, created_at \
                   FROM org_memory_audit WHERE organization_id=?1 \
                  ORDER BY created_at DESC, id DESC",
            )
            .map_err(backend)?;
        let rows = stmt
            .query_map([self.organization_id.as_str()], |row| {
                let action: String = row.get(3)?;
                let decision: String = row.get(5)?;
                let detail: Option<String> = row.get(6)?;
                let created_at: String = row.get(7)?;
                Ok(OrgMemoryAuditEntry {
                    id: row.get(0)?,
                    organization_id: self.organization_id.clone(),
                    memory_id: row.get(1)?,
                    version: row.get::<_, i64>(2)?.max(0) as u32,
                    action: OrgMemoryAuditAction::parse(&action).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            3,
                            "action".into(),
                            rusqlite::types::Type::Text,
                        )
                    })?,
                    actor_principal_id: row.get(4)?,
                    decision: OrgMemoryDecision::parse(&decision).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            5,
                            "decision".into(),
                            rusqlite::types::Type::Text,
                        )
                    })?,
                    detail_json: detail
                        .map(|value| serde_json::from_str::<serde_json::Value>(&value))
                        .transpose()
                        .map_err(|_| {
                            rusqlite::Error::InvalidColumnType(
                                6,
                                "detail_json".into(),
                                rusqlite::types::Type::Text,
                            )
                        })?,
                    created_at: parse_time(&created_at).map_err(|_| {
                        rusqlite::Error::InvalidColumnType(
                            7,
                            "created_at".into(),
                            rusqlite::types::Type::Text,
                        )
                    })?,
                })
            })
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        Ok(rows)
    }
}

fn row_to_entry(
    row: &rusqlite::Row<'_>,
    organization_id: &str,
) -> rusqlite::Result<OrgMemoryEntry> {
    let kind: String = row.get(2)?;
    let sensitivity: String = row.get(6)?;
    let status: String = row.get(7)?;
    let published_at: String = row.get(10)?;
    let revoked_at: Option<String> = row.get(11)?;
    Ok(OrgMemoryEntry {
        id: row.get(0)?,
        organization_id: organization_id.to_string(),
        version: row.get::<_, i64>(1)?.max(0) as u32,
        kind: MemoryKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(2, "kind".into(), rusqlite::types::Type::Text)
        })?,
        subject: row.get(3)?,
        content: row.get(4)?,
        confidence: row.get(5)?,
        sensitivity: MemorySensitivity::parse(&sensitivity).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(6, "sensitivity".into(), rusqlite::types::Type::Text)
        })?,
        status: OrgMemoryStatus::parse(&status).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(7, "status".into(), rusqlite::types::Type::Text)
        })?,
        created_by: row.get(8)?,
        approved_by: row.get(9)?,
        published_at: parse_time(&published_at).map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                10,
                "published_at".into(),
                rusqlite::types::Type::Text,
            )
        })?,
        revoked_at: revoked_at
            .as_deref()
            .map(parse_time)
            .transpose()
            .map_err(|_| {
                rusqlite::Error::InvalidColumnType(
                    11,
                    "revoked_at".into(),
                    rusqlite::types::Type::Text,
                )
            })?,
        revoked_by: row.get(12)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn write_audit(
    conn: &rusqlite::Connection,
    organization_id: &str,
    memory_id: &str,
    version: u32,
    action: OrgMemoryAuditAction,
    actor: &str,
    decision: OrgMemoryDecision,
    detail: Option<serde_json::Value>,
    created_at: &str,
) -> Result<(), OrgMemoryError> {
    conn.execute(
        "INSERT INTO org_memory_audit (\
           id, organization_id, memory_id, version, action, actor_principal_id, decision, \
           detail_json, created_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            Uuid::new_v4().to_string(),
            organization_id,
            memory_id,
            i64::from(version),
            action.as_str(),
            actor,
            decision.as_str(),
            detail
                .map(|value| serde_json::to_string(&value))
                .transpose()
                .map_err(backend)?,
            created_at,
        ],
    )
    .map_err(backend)?;
    Ok(())
}
