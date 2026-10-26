//! Reading and writing `query_history`.
//!
//! Two read paths exist on purpose. `get_query_history` is the simple
//! connection/database/schema filter used by narrow callers; `query_history_page`
//! adds free-text search, a time window, a sort order, and — the part that
//! matters — a `total` that counts the *whole* match set rather than the page,
//! so a truncated page can never read as "that's all there is".

use super::{
    order_by_sql, HistoryDb, HistoryDbError, QueryHistoryEntry, QueryHistoryFilter,
    QueryHistoryPage, MAX_QUERY_HISTORY,
};
use chrono::{DateTime, Utc};
use rusqlite::params;

impl HistoryDb {
    /// Append one query outcome to `history.sqlite` (plaintext `sql` column).
    ///
    /// See the module-level security note: history is not encrypted at rest.
    pub fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            let dominated: Option<String> = match conn.query_row(
                "SELECT sql FROM query_history WHERE connection_id = ?1 AND database = ?2 AND schema IS ?3 ORDER BY executed_at DESC LIMIT 1",
                params![entry.connection_id, entry.database, entry.schema],
                |row| row.get(0),
            ) {
                Ok(v) => Some(v),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(e) => return Err(HistoryDbError::from(e)),
            };

            if dominated
                .as_deref()
                .map(|sql| sql.trim() == entry.sql.trim())
                .unwrap_or(false)
            {
                conn.execute(
                    "UPDATE query_history SET
                        executed_at = ?1,
                        execution_time_ms = ?2,
                        rows_affected = ?3,
                        success = ?4,
                        error_message = ?5
                     WHERE id = (
                        SELECT id FROM query_history WHERE connection_id = ?6 AND database = ?7 AND schema IS ?8 ORDER BY executed_at DESC LIMIT 1
                     )",
                    params![
                        entry.executed_at.to_rfc3339(),
                        entry.execution_time_ms as i64,
                        entry.rows_affected.map(|v| v as i64),
                        entry.success as i32,
                        entry.error_message,
                        entry.connection_id,
                        entry.database,
                        entry.schema,
                    ],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO query_history (
                        id, connection_id, database, schema, sql, executed_at,
                        execution_time_ms, rows_affected, success, error_message
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        entry.id,
                        entry.connection_id,
                        entry.database,
                        entry.schema,
                        entry.sql,
                        entry.executed_at.to_rfc3339(),
                        entry.execution_time_ms as i64,
                        entry.rows_affected.map(|v| v as i64),
                        entry.success as i32,
                        entry.error_message,
                    ],
                )?;
            }

            trim_query_history(conn)?;
            Ok(())
        })
    }

    pub fn get_query_history(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryDbError> {
        self.with_conn(|conn| {
            let mut where_clauses: Vec<String> = Vec::new();
            let mut filter_params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(cid) = connection_id {
                where_clauses.push(format!("connection_id = ?{}", filter_params.len() + 1));
                filter_params.push(Box::new(cid.to_string()));
            }
            if let Some(db) = database {
                where_clauses.push(format!("database = ?{}", filter_params.len() + 1));
                filter_params.push(Box::new(db.to_string()));
            }
            if let Some(s) = schema {
                // Empty string means "rows with no schema" (NULL); else exact match.
                if s.is_empty() {
                    where_clauses.push("schema IS NULL".to_string());
                } else {
                    where_clauses.push(format!("schema = ?{}", filter_params.len() + 1));
                    filter_params.push(Box::new(s.to_string()));
                }
            }
            let where_sql = if where_clauses.is_empty() {
                String::new()
            } else {
                format!("WHERE {}", where_clauses.join(" AND "))
            };
            let sql = format!(
                "SELECT id, connection_id, database, schema, sql, executed_at, \
                 execution_time_ms, rows_affected, success, error_message \
                 FROM query_history {} ORDER BY executed_at DESC LIMIT ?{}",
                where_sql,
                filter_params.len() + 1,
            );
            let mut stmt = conn.prepare(&sql)?;
            filter_params.push(Box::new(limit as i64));
            let rows = stmt.query_map(
                rusqlite::params_from_iter(filter_params.iter().map(|p| p.as_ref())),
                map_query_row,
            )?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(HistoryDbError::from)
        })
    }

    pub fn clear_query_history(&self) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM query_history", [])?;
            Ok(())
        })
    }

    /// Delete exactly one history row. Returns the number of rows removed, which
    /// is `0` for an unknown id — callers can tell a no-op from a real delete
    /// instead of assuming success.
    pub fn delete_query_history(&self, id: &str) -> Result<u64, HistoryDbError> {
        self.with_conn(|conn| {
            let deleted = conn.execute("DELETE FROM query_history WHERE id = ?1", params![id])?;
            Ok(deleted as u64)
        })
    }

    /// Build the shared `WHERE` fragment for both the page and the total count.
    ///
    /// Kept in one place so the count and the page can never drift apart: a
    /// count computed from different predicates than the rows it describes is
    /// how a "showing N of M" label starts lying.
    fn history_predicate(
        filter: &QueryHistoryFilter<'_>,
    ) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(cid) = filter.connection_id {
            clauses.push(format!("connection_id = ?{}", params.len() + 1));
            params.push(Box::new(cid.to_string()));
        }
        if let Some(db) = filter.database {
            clauses.push(format!("database = ?{}", params.len() + 1));
            params.push(Box::new(db.to_string()));
        }
        if let Some(s) = filter.schema {
            // Empty string means "rows with no schema" (NULL); else exact match.
            if s.is_empty() {
                clauses.push("schema IS NULL".to_string());
            } else {
                clauses.push(format!("schema = ?{}", params.len() + 1));
                params.push(Box::new(s.to_string()));
            }
        }
        if let Some(needle) = filter.search {
            let needle = needle.trim();
            if !needle.is_empty() {
                // ESCAPE so a literal % or _ in the user's needle is not a
                // wildcard. Matches the old client-side `includes()` semantics
                // closely enough while narrowing to the two indexed-ish columns.
                let idx = params.len() + 1;
                clauses.push(format!(
                    "(LOWER(sql) LIKE ?{idx} ESCAPE '\\' OR LOWER(database) LIKE ?{idx} ESCAPE '\\' \
                      OR LOWER(COALESCE(schema, '')) LIKE ?{idx} ESCAPE '\\')"
                ));
                let pattern = format!(
                    "%{}%",
                    needle
                        .to_lowercase()
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                );
                params.push(Box::new(pattern));
            }
        }
        if let Some(since) = filter.since {
            clauses.push(format!("executed_at >= ?{}", params.len() + 1));
            params.push(Box::new(since.to_string()));
        }
        if let Some(until) = filter.until {
            clauses.push(format!("executed_at <= ?{}", params.len() + 1));
            params.push(Box::new(until.to_string()));
        }

        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        (where_sql, params)
    }

    /// Paged read with an honest total. See [`QueryHistoryPage`].
    pub fn query_history_page(
        &self,
        filter: &QueryHistoryFilter<'_>,
    ) -> Result<QueryHistoryPage, HistoryDbError> {
        let (where_sql, params) = Self::history_predicate(filter);
        self.with_conn(|conn| {
            // The count reuses the identical predicate and binds no LIMIT, so
            // `total` is the size of the full match set.
            let total: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM query_history {where_sql}"),
                rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
                |r| r.get(0),
            )?;

            let mut bound = params;
            bound.push(Box::new(filter.limit as i64));
            let sql = format!(
                "SELECT id, connection_id, database, schema, sql, executed_at, \
                 execution_time_ms, rows_affected, success, error_message \
                 FROM query_history {where_sql} ORDER BY {} LIMIT ?{}",
                order_by_sql(filter.order),
                bound.len(),
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(bound.iter().map(|p| p.as_ref())),
                map_query_row,
            )?;
            let entries = rows.collect::<Result<Vec<_>, _>>()?;

            Ok(QueryHistoryPage {
                entries,
                total: total.max(0) as u64,
            })
        })
    }
}

pub(super) fn map_query_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryEntry> {
    let executed_at: String = row.get(5)?;
    let executed_at = DateTime::parse_from_rfc3339(&executed_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let rows_affected: Option<i64> = row.get(7)?;
    Ok(QueryHistoryEntry {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        database: row.get(2)?,
        schema: row.get(3)?,
        sql: row.get(4)?,
        executed_at,
        execution_time_ms: row.get::<_, i64>(6)? as u64,
        rows_affected: rows_affected.map(|v| v as u64),
        success: row.get::<_, i32>(8)? != 0,
        error_message: row.get(9)?,
    })
}

pub(super) fn trim_query_history(conn: &rusqlite::Connection) -> Result<(), HistoryDbError> {
    conn.execute(
        "DELETE FROM query_history WHERE id NOT IN (
            SELECT id FROM query_history ORDER BY executed_at DESC LIMIT ?1
         )",
        params![MAX_QUERY_HISTORY as i64],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "query_history_tests.rs"]
mod query_history_tests;
