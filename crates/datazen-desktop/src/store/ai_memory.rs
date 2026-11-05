//! AppDb v7 storage for AI connection notes, memories, FTS search, and audit.
//!
//! The tables deliberately keep snake_case columns and no IPC DTOs: T10/T12
//! own the camelCase boundary. All writes are synchronous SQLite calls under
//! the AppDb connection mutex. `secret` memories are rejected at this boundary
//! rather than stored and filtered later (PRD §6.9).
//!
//! The external-content FTS table is synchronised by the three triggers in the
//! v7 migration, so these methods never write `ai_memories_fts` directly.

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::{AppDb, AppDbError};

const SCOPE_TYPES: &[&str] = &[
    "user",
    "connection",
    "database",
    "schema",
    "table",
    "column",
    "org",
];
const KINDS: &[&str] = &[
    "preference",
    "glossary",
    "relationship",
    "pattern",
    "correction",
    "environment",
    "negative",
    "schema_note",
];
const SOURCES: &[&str] = &[
    "explicit",
    "correction",
    "usage",
    "feedback",
    "schema_note",
    "import",
    "org",
];
const SENSITIVITIES: &[&str] = &["normal", "internal", "secret"];
const STATUSES: &[&str] = &["candidate", "active", "rejected", "archived", "stale"];
const AUDIT_ACTIONS: &[&str] = &[
    "create", "update", "confirm", "reject", "archive", "delete", "inject", "feedback",
];

const MEMORY_COLUMNS: &str = "m.id, m.organization_id, m.owner_principal_id, m.scope_type, \
    m.scope_id, m.connection_id, m.database_name, m.schema_name, m.table_name, m.column_name, \
    m.kind, m.subject, m.content, m.structured_json, m.source, m.source_ref, m.confidence, \
    m.sensitivity, m.status, m.pinned, m.use_count, m.last_used_at, m.expires_at, \
    m.content_hash, m.created_at, m.updated_at";

/// The persisted shape of one `ai_connection_notes` row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiConnectionNoteRecord {
    pub connection_id: String,
    pub organization_id: String,
    pub owner_principal_id: String,
    pub content: String,
    pub updated_at: String,
}

/// The persisted shape of one `ai_memories` row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiMemoryRecord {
    pub id: String,
    pub organization_id: String,
    pub owner_principal_id: String,
    pub scope_type: String,
    pub scope_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub kind: String,
    pub subject: String,
    pub content: String,
    pub structured_json: Option<String>,
    pub source: String,
    pub source_ref: Option<String>,
    pub confidence: f64,
    pub sensitivity: String,
    pub status: String,
    pub pinned: bool,
    pub use_count: i64,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub content_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Counts shown before deleting a connection: one note at most, N memories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiConnectionContextCounts {
    pub note_count: i64,
    pub memory_count: i64,
}

/// The persisted shape of one `ai_memory_audit` row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiMemoryAuditRecord {
    pub id: String,
    pub memory_id: String,
    pub action: String,
    pub actor_principal_id: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub detail_json: Option<String>,
    pub created_at: String,
}

impl AppDb {
    // ── Connection notes ──────────────────────────────────────────────────

    pub(crate) fn upsert_ai_connection_note(
        &self,
        note: &AiConnectionNoteRecord,
    ) -> Result<(), AppDbError> {
        if note.content.len() > datazen::ai::MAX_CONNECTION_NOTES_BYTES {
            return Err(AppDbError::Validation(format!(
                "connection notes exceed {} bytes",
                datazen::ai::MAX_CONNECTION_NOTES_BYTES
            )));
        }
        self.with_conn(|conn| {
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT organization_id, owner_principal_id FROM ai_connection_notes \
                      WHERE connection_id=?1",
                    [note.connection_id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((organization_id, owner_principal_id)) = existing {
                if organization_id != note.organization_id
                    || owner_principal_id != note.owner_principal_id
                {
                    return Err(AppDbError::Validation(
                        "an existing connection note cannot change organization or owner".into(),
                    ));
                }
            }
            conn.execute(
                "INSERT INTO ai_connection_notes \
                   (connection_id, organization_id, owner_principal_id, content, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT(connection_id) DO UPDATE SET \
                   organization_id=excluded.organization_id, \
                   owner_principal_id=excluded.owner_principal_id, \
                   content=excluded.content, \
                   updated_at=excluded.updated_at",
                params![
                    note.connection_id,
                    note.organization_id,
                    note.owner_principal_id,
                    note.content,
                    note.updated_at
                ],
            )?;
            Ok(())
        })
    }

    /// Write one connection note with the user-facing 4KB cap. Empty or
    /// whitespace-only content deletes the row instead of persisting it.
    pub(crate) fn set_ai_connection_note(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        connection_id: &str,
        content: &str,
    ) -> Result<(), AppDbError> {
        if content.len() > datazen::ai::MAX_CONNECTION_NOTES_BYTES {
            return Err(AppDbError::Validation(format!(
                "connection notes exceed {} bytes",
                datazen::ai::MAX_CONNECTION_NOTES_BYTES
            )));
        }
        if content.trim().is_empty() {
            self.delete_ai_connection_note(organization_id, owner_principal_id, connection_id)?;
            return Ok(());
        }
        self.upsert_ai_connection_note(&AiConnectionNoteRecord {
            connection_id: connection_id.to_string(),
            organization_id: organization_id.to_string(),
            owner_principal_id: owner_principal_id.to_string(),
            content: content.to_string(),
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    pub(crate) fn get_ai_connection_note(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        connection_id: &str,
    ) -> Result<Option<AiConnectionNoteRecord>, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT connection_id, organization_id, owner_principal_id, content, updated_at \
                   FROM ai_connection_notes \
                  WHERE connection_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                params![connection_id, organization_id, owner_principal_id],
                |row| {
                    Ok(AiConnectionNoteRecord {
                        connection_id: row.get(0)?,
                        organization_id: row.get(1)?,
                        owner_principal_id: row.get(2)?,
                        content: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub(crate) fn delete_ai_connection_note(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        connection_id: &str,
    ) -> Result<bool, AppDbError> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM ai_connection_notes \
                  WHERE connection_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                params![connection_id, organization_id, owner_principal_id],
            )? > 0)
        })
    }

    /// Counts for the delete-connection confirmation. One note max; memories
    /// are counted by both the explicit connection id and the legacy
    /// connection-scoped predicate used by the purge.
    pub(crate) fn count_ai_connection_context(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        connection_id: &str,
    ) -> Result<AiConnectionContextCounts, AppDbError> {
        self.with_conn(|conn| {
            let note_count = conn.query_row(
                "SELECT COUNT(*) FROM ai_connection_notes \
                  WHERE connection_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                params![connection_id, organization_id, owner_principal_id],
                |row| row.get(0),
            )?;
            let memory_count = conn.query_row(
                "SELECT COUNT(*) FROM ai_memories \
                  WHERE organization_id=?1 \
                    AND (connection_id=?2 OR (scope_type='connection' AND scope_id=?2))",
                params![organization_id, connection_id],
                |row| row.get(0),
            )?;
            Ok(AiConnectionContextCounts {
                note_count,
                memory_count,
            })
        })
    }

    // ── Memories ──────────────────────────────────────────────────────────

    pub(crate) fn upsert_ai_memory(&self, memory: &AiMemoryRecord) -> Result<(), AppDbError> {
        validate_memory(memory)?;
        self.with_conn(|conn| {
            // A memory id is globally unique. An update may change content and
            // scope metadata, but it must never re-home an existing row to a
            // different organization or owner (that would defeat every scoped
            // read/search below).
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT organization_id, owner_principal_id FROM ai_memories WHERE id=?1",
                    [memory.id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((organization_id, owner_principal_id)) = existing {
                if organization_id != memory.organization_id
                    || owner_principal_id != memory.owner_principal_id
                {
                    return Err(AppDbError::Validation(
                        "an existing AI memory cannot change organization or owner".into(),
                    ));
                }
            }
            let changed = conn.execute(
                "INSERT INTO ai_memories (\
                   id, organization_id, owner_principal_id, scope_type, scope_id, connection_id, \
                   database_name, schema_name, table_name, column_name, kind, subject, content, \
                   structured_json, source, source_ref, confidence, sensitivity, status, pinned, \
                   use_count, last_used_at, expires_at, content_hash, created_at, updated_at\
                 ) VALUES (\
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                   ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26\
                 ) ON CONFLICT(id) DO UPDATE SET \
                   organization_id=excluded.organization_id, \
                   owner_principal_id=excluded.owner_principal_id, \
                   scope_type=excluded.scope_type, \
                   scope_id=excluded.scope_id, \
                   connection_id=excluded.connection_id, \
                   database_name=excluded.database_name, \
                   schema_name=excluded.schema_name, \
                   table_name=excluded.table_name, \
                   column_name=excluded.column_name, \
                   kind=excluded.kind, \
                   subject=excluded.subject, \
                   content=excluded.content, \
                   structured_json=excluded.structured_json, \
                   source=excluded.source, \
                   source_ref=excluded.source_ref, \
                   confidence=excluded.confidence, \
                   sensitivity=excluded.sensitivity, \
                   status=excluded.status, \
                   pinned=excluded.pinned, \
                   use_count=excluded.use_count, \
                   last_used_at=excluded.last_used_at, \
                   expires_at=excluded.expires_at, \
                   content_hash=excluded.content_hash, \
                   updated_at=excluded.updated_at \
                 WHERE ai_memories.organization_id=excluded.organization_id \
                   AND ai_memories.owner_principal_id=excluded.owner_principal_id",
                params![
                    memory.id,
                    memory.organization_id,
                    memory.owner_principal_id,
                    memory.scope_type,
                    memory.scope_id,
                    memory.connection_id,
                    memory.database_name,
                    memory.schema_name,
                    memory.table_name,
                    memory.column_name,
                    memory.kind,
                    memory.subject,
                    memory.content,
                    memory.structured_json,
                    memory.source,
                    memory.source_ref,
                    memory.confidence,
                    memory.sensitivity,
                    memory.status,
                    if memory.pinned { 1_i64 } else { 0 },
                    memory.use_count,
                    memory.last_used_at,
                    memory.expires_at,
                    memory.content_hash,
                    memory.created_at,
                    memory.updated_at
                ],
            )?;
            // The WHERE above makes a cross-owner conflict a no-op instead of
            // a re-home; treat that as a validation failure even if the
            // pre-check raced with another writer.
            if changed == 0 {
                return Err(AppDbError::Validation(
                    "an existing AI memory cannot change organization or owner".into(),
                ));
            }
            Ok(())
        })
    }

    pub(crate) fn get_ai_memory(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        id: &str,
    ) -> Result<Option<AiMemoryRecord>, AppDbError> {
        self.with_conn(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {MEMORY_COLUMNS} FROM ai_memories m \
                      WHERE m.id=?1 AND m.organization_id=?2 AND m.owner_principal_id=?3"
                ),
                params![id, organization_id, owner_principal_id],
                memory_from_row,
            )
            .optional()
            .map_err(Into::into)
        })
    }

    pub(crate) fn list_ai_memories(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
    ) -> Result<Vec<AiMemoryRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&format!(
                "SELECT {MEMORY_COLUMNS} FROM ai_memories m \
                  WHERE m.organization_id=?1 AND m.owner_principal_id=?2 \
                  ORDER BY m.updated_at DESC, m.id"
            ))?;
            let rows = statement.query_map(
                params![organization_id, owner_principal_id],
                memory_from_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// FTS5 search scoped to the caller's organization and principal. The
    /// caller supplies FTS5 query syntax; an empty query returns no rows.
    pub(crate) fn search_ai_memories(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<AiMemoryRecord>, AppDbError> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 100);
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&format!(
                "SELECT {MEMORY_COLUMNS} FROM ai_memories m \
                   JOIN ai_memories_fts f ON f.rowid = m.rowid \
                  WHERE ai_memories_fts MATCH ?1 \
                    AND m.organization_id=?2 AND m.owner_principal_id=?3 \
                  ORDER BY bm25(ai_memories_fts), m.updated_at DESC, m.id \
                  LIMIT ?4"
            ))?;
            let rows = statement.query_map(
                params![query, organization_id, owner_principal_id, i64::from(limit)],
                memory_from_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    pub(crate) fn delete_ai_memory(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        id: &str,
    ) -> Result<bool, AppDbError> {
        self.with_conn(|conn| {
            Ok(conn.execute(
                "DELETE FROM ai_memories \
                  WHERE id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                params![id, organization_id, owner_principal_id],
            )? > 0)
        })
    }

    // ── Audit ─────────────────────────────────────────────────────────────

    pub(crate) fn append_ai_memory_audit(
        &self,
        audit: &AiMemoryAuditRecord,
    ) -> Result<(), AppDbError> {
        ensure_allowed("action", &audit.action, AUDIT_ACTIONS)?;
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO ai_memory_audit \
                   (id, memory_id, action, actor_principal_id, before_hash, after_hash, \
                    detail_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    audit.id,
                    audit.memory_id,
                    audit.action,
                    audit.actor_principal_id,
                    audit.before_hash,
                    audit.after_hash,
                    audit.detail_json,
                    audit.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub(crate) fn list_ai_memory_audit(
        &self,
        memory_id: &str,
    ) -> Result<Vec<AiMemoryAuditRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT id, memory_id, action, actor_principal_id, before_hash, after_hash, \
                        detail_json, created_at \
                   FROM ai_memory_audit WHERE memory_id=?1 ORDER BY created_at, id",
            )?;
            let rows = statement.query_map([memory_id], |row| {
                Ok(AiMemoryAuditRecord {
                    id: row.get(0)?,
                    memory_id: row.get(1)?,
                    action: row.get(2)?,
                    actor_principal_id: row.get(3)?,
                    before_hash: row.get(4)?,
                    after_hash: row.get(5)?,
                    detail_json: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    // ── Connection deletion cleanup ───────────────────────────────────────

    /// Remove every AI note/memory/audit row owned by a deleted connection.
    ///
    /// One `Immediate` transaction covers all three tables. The audit table
    /// has no foreign key, so audit rows are removed by `memory_id` before the
    /// memories themselves (the FTS delete trigger runs on the memory delete).
    pub(crate) fn purge_connection_ai_data(
        &self,
        connection_id: &str,
        organization_id: &str,
    ) -> Result<(), AppDbError> {
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let memory_predicate = "organization_id=?1 AND \
                (connection_id=?2 OR (scope_type='connection' AND scope_id=?2))";
            tx.execute(
                &format!(
                    "DELETE FROM ai_memory_audit WHERE memory_id IN \
                       (SELECT id FROM ai_memories WHERE {memory_predicate})"
                ),
                params![organization_id, connection_id],
            )?;
            tx.execute(
                &format!("DELETE FROM ai_memories WHERE {memory_predicate}"),
                params![organization_id, connection_id],
            )?;
            tx.execute(
                "DELETE FROM ai_connection_notes \
                  WHERE organization_id=?1 AND connection_id=?2",
                params![organization_id, connection_id],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}

fn validate_memory(memory: &AiMemoryRecord) -> Result<(), AppDbError> {
    ensure_allowed("scope_type", &memory.scope_type, SCOPE_TYPES)?;
    ensure_allowed("kind", &memory.kind, KINDS)?;
    ensure_allowed("source", &memory.source, SOURCES)?;
    ensure_allowed("sensitivity", &memory.sensitivity, SENSITIVITIES)?;
    ensure_allowed("status", &memory.status, STATUSES)?;
    if memory.sensitivity == "secret" {
        return Err(AppDbError::Validation(
            "secret AI memories are rejected at the storage boundary".into(),
        ));
    }
    if !memory.confidence.is_finite() || !(0.0..=1.0).contains(&memory.confidence) {
        return Err(AppDbError::Validation(format!(
            "confidence must be finite and between 0 and 1, got {}",
            memory.confidence
        )));
    }
    if memory.use_count < 0 {
        return Err(AppDbError::Validation(
            "use_count must not be negative".into(),
        ));
    }
    Ok(())
}

fn ensure_allowed(field: &str, value: &str, allowed: &[&str]) -> Result<(), AppDbError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AppDbError::Validation(format!(
            "invalid AI memory {field}: {value}"
        )))
    }
}

fn memory_from_row(row: &Row<'_>) -> rusqlite::Result<AiMemoryRecord> {
    Ok(AiMemoryRecord {
        id: row.get(0)?,
        organization_id: row.get(1)?,
        owner_principal_id: row.get(2)?,
        scope_type: row.get(3)?,
        scope_id: row.get(4)?,
        connection_id: row.get(5)?,
        database_name: row.get(6)?,
        schema_name: row.get(7)?,
        table_name: row.get(8)?,
        column_name: row.get(9)?,
        kind: row.get(10)?,
        subject: row.get(11)?,
        content: row.get(12)?,
        structured_json: row.get(13)?,
        source: row.get(14)?,
        source_ref: row.get(15)?,
        confidence: row.get(16)?,
        sensitivity: row.get(17)?,
        status: row.get(18)?,
        pinned: row.get::<_, i64>(19)? != 0,
        use_count: row.get(20)?,
        last_used_at: row.get(21)?,
        expires_at: row.get(22)?,
        content_hash: row.get(23)?,
        created_at: row.get(24)?,
        updated_at: row.get(25)?,
    })
}

#[cfg(test)]
#[path = "ai_memory_tests.rs"]
mod tests;
