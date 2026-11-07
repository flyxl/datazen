//! AppDb v8 storage for AI chat sessions and their messages.
//!
//! A session is scoped by organization + owner; `session_key` is the frontend
//! isolation key (connection/database). Multiple sessions may share a key so
//! the history list can navigate past conversations. Messages are replaced or
//! appended explicitly by the caller; deleting a session cascades its messages.

use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{AppDb, AppDbError};

const ROLES: &[&str] = &["user", "assistant", "tool", "system"];
const STATUSES: &[&str] = &["complete", "cancelled", "failed"];

const SESSION_COLUMNS: &str = "s.id, s.organization_id, s.owner_principal_id, s.session_key, \
    s.connection_id, s.database_name, s.title, s.metadata_json, s.created_at, s.updated_at, \
    s.last_access_at";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSessionRecord {
    pub id: String,
    pub organization_id: String,
    pub owner_principal_id: String,
    pub session_key: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub title: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_access_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiSessionMessageRecord {
    pub id: String,
    pub session_id: String,
    pub position: i64,
    pub role: String,
    pub content: String,
    pub status: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiSessionSummary {
    pub session: AiSessionRecord,
    pub message_count: i64,
    pub preview: String,
}

impl AppDb {
    pub(crate) fn list_ai_sessions(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        connection_id: Option<&str>,
        database_name: Option<&str>,
        session_key: Option<&str>,
    ) -> Result<Vec<AiSessionSummary>, AppDbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&format!(
                "SELECT {SESSION_COLUMNS}, \
                   (SELECT COUNT(*) FROM ai_session_messages m WHERE m.session_id=s.id), \
                   COALESCE((SELECT m.content FROM ai_session_messages m \
                              WHERE m.session_id=s.id AND m.role='user' \
                              ORDER BY m.position LIMIT 1), '') \
                 FROM ai_sessions s \
                 WHERE s.organization_id=?1 AND s.owner_principal_id=?2 \
                   AND (?3 IS NULL OR COALESCE(s.connection_id,'')=?3) \
                   AND (?4 IS NULL OR COALESCE(s.database_name,'')=?4) \
                   AND (?5 IS NULL OR s.session_key=?5) \
                 ORDER BY s.last_access_at DESC, s.id"
            ))?;
            let rows = statement.query_map(
                params![
                    organization_id,
                    owner_principal_id,
                    connection_id,
                    database_name,
                    session_key
                ],
                |row| {
                    Ok(AiSessionSummary {
                        session: session_from_row(row, 0)?,
                        message_count: row.get(11)?,
                        preview: row.get(12)?,
                    })
                },
            )?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    pub(crate) fn get_ai_session(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        id: &str,
    ) -> Result<Option<AiSessionRecord>, AppDbError> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row(
                    &format!(
                        "SELECT {SESSION_COLUMNS} FROM ai_sessions s \
                          WHERE s.id=?1 AND s.organization_id=?2 AND s.owner_principal_id=?3"
                    ),
                    params![id, organization_id, owner_principal_id],
                    |row| session_from_row(row, 0),
                )
                .optional()?)
        })
    }

    pub(crate) fn list_ai_session_messages(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        session_id: &str,
    ) -> Result<Vec<AiSessionMessageRecord>, AppDbError> {
        self.with_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT m.id, m.session_id, m.position, m.role, m.content, m.status, \
                        m.metadata_json, m.created_at \
                   FROM ai_session_messages m \
                   JOIN ai_sessions s ON s.id=m.session_id \
                  WHERE m.session_id=?1 AND s.organization_id=?2 AND s.owner_principal_id=?3 \
                  ORDER BY m.position",
            )?;
            let rows = statement.query_map(
                params![session_id, organization_id, owner_principal_id],
                message_from_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// Replace or append a session's messages in one transaction. The session
    /// row and message rows are written together so a failure never leaves a
    /// half-replaced conversation.
    pub(crate) fn upsert_ai_session(
        &self,
        mut session: AiSessionRecord,
        messages: &[AiSessionMessageRecord],
        append: bool,
    ) -> Result<AiSessionRecord, AppDbError> {
        validate_session(&session)?;
        for message in messages {
            validate_message(message)?;
        }
        self.with_conn_mut(|conn| {
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let existing: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT organization_id, owner_principal_id, created_at \
                       FROM ai_sessions WHERE id=?1",
                    [session.id.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((organization_id, owner_principal_id, created_at)) = existing {
                if organization_id != session.organization_id
                    || owner_principal_id != session.owner_principal_id
                {
                    return Err(AppDbError::Validation(
                        "an existing AI session cannot change organization or owner".into(),
                    ));
                }
                session.created_at = created_at;
            } else if session.created_at.trim().is_empty() {
                session.created_at = Utc::now().to_rfc3339();
            }
            if session.updated_at.trim().is_empty() {
                session.updated_at = Utc::now().to_rfc3339();
            }
            if session.last_access_at.trim().is_empty() {
                session.last_access_at = session.updated_at.clone();
            }
            tx.execute(
                "INSERT INTO ai_sessions (\
                   id, organization_id, owner_principal_id, session_key, connection_id, \
                   database_name, title, metadata_json, created_at, updated_at, last_access_at\
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) \
                 ON CONFLICT(id) DO UPDATE SET \
                   session_key=excluded.session_key, \
                   connection_id=excluded.connection_id, \
                   database_name=excluded.database_name, \
                   title=excluded.title, \
                   metadata_json=excluded.metadata_json, \
                   updated_at=excluded.updated_at, \
                   last_access_at=excluded.last_access_at \
                 WHERE ai_sessions.organization_id=excluded.organization_id \
                   AND ai_sessions.owner_principal_id=excluded.owner_principal_id",
                params![
                    session.id,
                    session.organization_id,
                    session.owner_principal_id,
                    session.session_key,
                    session.connection_id,
                    session.database_name,
                    session.title,
                    session.metadata_json,
                    session.created_at,
                    session.updated_at,
                    session.last_access_at
                ],
            )?;
            let mut next_position: i64 = if append {
                tx.query_row(
                    "SELECT COALESCE(MAX(position), -1) + 1 FROM ai_session_messages WHERE session_id=?1",
                    [session.id.as_str()],
                    |row| row.get(0),
                )?
            } else {
                tx.execute(
                    "DELETE FROM ai_session_messages WHERE session_id=?1",
                    [session.id.as_str()],
                )?;
                0
            };
            for message in messages {
                tx.execute(
                    "INSERT OR IGNORE INTO ai_session_messages (\
                       id, session_id, position, role, content, status, metadata_json, created_at\
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        message.id,
                        session.id,
                        next_position,
                        message.role,
                        message.content,
                        message.status,
                        message.metadata_json,
                        message.created_at
                    ],
                )?;
                next_position += 1;
            }
            tx.commit()?;
            Ok(session)
        })
    }

    pub(crate) fn delete_ai_session(
        &self,
        organization_id: &str,
        owner_principal_id: &str,
        id: &str,
    ) -> Result<bool, AppDbError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "DELETE FROM ai_sessions WHERE id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                params![id, organization_id, owner_principal_id],
            )?;
            Ok(changed > 0)
        })
    }

    /// Counts rows in a session's message table; used by cascade tests and by
    /// callers that need a cheap integrity check without loading metadata.
    #[cfg(test)]
    pub(crate) fn ai_session_message_count(&self, session_id: &str) -> Result<i64, AppDbError> {
        self.with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM ai_session_messages WHERE session_id=?1",
                [session_id],
                |row| row.get(0),
            )?)
        })
    }
}

fn session_from_row(row: &Row<'_>, offset: usize) -> rusqlite::Result<AiSessionRecord> {
    Ok(AiSessionRecord {
        id: row.get(offset)?,
        organization_id: row.get(offset + 1)?,
        owner_principal_id: row.get(offset + 2)?,
        session_key: row.get(offset + 3)?,
        connection_id: row.get(offset + 4)?,
        database_name: row.get(offset + 5)?,
        title: row.get(offset + 6)?,
        metadata_json: row.get(offset + 7)?,
        created_at: row.get(offset + 8)?,
        updated_at: row.get(offset + 9)?,
        last_access_at: row.get(offset + 10)?,
    })
}

fn message_from_row(row: &Row<'_>) -> rusqlite::Result<AiSessionMessageRecord> {
    Ok(AiSessionMessageRecord {
        id: row.get(0)?,
        session_id: row.get(1)?,
        position: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        status: row.get(5)?,
        metadata_json: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn validate_session(session: &AiSessionRecord) -> Result<(), AppDbError> {
    if session.id.trim().is_empty() {
        return Err(AppDbError::Validation("AI session id is required".into()));
    }
    if session.session_key.trim().is_empty() {
        return Err(AppDbError::Validation("AI session key is required".into()));
    }
    if session.organization_id.trim().is_empty() || session.owner_principal_id.trim().is_empty() {
        return Err(AppDbError::Validation(
            "AI session scope is required".into(),
        ));
    }
    Ok(())
}

fn validate_message(message: &AiSessionMessageRecord) -> Result<(), AppDbError> {
    if message.id.trim().is_empty() {
        return Err(AppDbError::Validation("AI message id is required".into()));
    }
    if !ROLES.contains(&message.role.as_str()) {
        return Err(AppDbError::Validation(format!(
            "unsupported AI message role: {}",
            message.role
        )));
    }
    if !STATUSES.contains(&message.status.as_str()) {
        return Err(AppDbError::Validation(format!(
            "unsupported AI message status: {}",
            message.status
        )));
    }
    if message.created_at.trim().is_empty() {
        return Err(AppDbError::Validation(
            "AI message timestamp is required".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "ai_sessions_tests.rs"]
mod tests;
