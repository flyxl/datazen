//! Reading out the retired `favorite_queries` table.
//!
//! Favorites moved to plain `.sql` files under the favorites root, so nothing
//! here writes: the table is read once, exported, then renamed out of the way.
//! It is never created again — an install that reaches v4 without one must
//! still finish migrating, and a `CREATE TABLE` here would hand every new
//! install a table the app has no writer for.
//!
//! The archive step uses `ALTER ... RENAME` rather than `DROP`, which is what
//! makes the migration reversible: the rows stay in the file, just under a name
//! no code path reads.

use super::{HistoryDb, HistoryDbError, LEGACY_FAVORITES_ARCHIVE_TABLE, LEGACY_FAVORITES_TABLE};
use chrono::{DateTime, Utc};

use super::schema::has_column;
use super::table_exists;

use crate::store::favorites::migrate::LegacyFavoriteRow;

impl HistoryDb {
    /// Read every row still sitting in `favorite_queries`.
    ///
    /// `None` means the table is absent — a fresh install, or an install that
    /// already exported and archived. Both are a no-op, so the common case
    /// costs one `sqlite_master` probe per launch.
    pub fn read_legacy_favorites(&self) -> Result<Option<Vec<LegacyFavoriteRow>>, HistoryDbError> {
        self.with_conn(|conn| {
            if !table_exists(conn, LEGACY_FAVORITES_TABLE)? {
                return Ok(None);
            }
            // A v1-era database reached without passing through the v2/v4
            // renames still calls the column `config_id`.
            let connection_col = if has_column(conn, LEGACY_FAVORITES_TABLE, "connection_id")? {
                "connection_id"
            } else {
                "config_id"
            };
            let sql = format!(
                "SELECT id, {connection_col}, title, sql, created_at FROM {LEGACY_FAVORITES_TABLE}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], map_legacy_favorite_row)?;
            let parsed = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(HistoryDbError::from)?;
            Ok(Some(parsed))
        })
    }

    /// Whether the retired table has already been archived under its v1 name.
    ///
    /// This — not the marker file, and not the absence of an error — is what
    /// "the export already ran" means. `migrate_legacy_favorites` consults it
    /// *before* reading any row, so a database that somehow holds a live
    /// `favorite_queries` next to an existing archive is recognised as already
    /// migrated instead of being re-exported on every launch.
    pub fn legacy_favorites_archived(&self) -> Result<bool, HistoryDbError> {
        self.with_conn(|conn| table_exists(conn, LEGACY_FAVORITES_ARCHIVE_TABLE))
    }

    /// Rename `favorite_queries` out of the way, keeping its rows.
    ///
    /// An `ALTER ... RENAME` is preferred over a `DROP` (this is the
    /// "保留原库备份" of plan §2.6.6) and over copying a live SQLite file: the
    /// rename is transactional, so either the table is archived or the export
    /// is retried next launch — never both half-done.
    ///
    /// Callers must have checked [`Self::legacy_favorites_archived`] first. The
    /// guard below is a last-resort no-op, not an expected path: when both
    /// names exist the live table is *not* pre-migration data (nothing in this
    /// code leaves both behind — the rename is atomic), so archiving it now
    /// would destroy rows this method has no reason to touch.
    pub fn archive_legacy_favorites_table(&self) -> Result<(), HistoryDbError> {
        self.with_conn(|conn| {
            if !table_exists(conn, LEGACY_FAVORITES_TABLE)? {
                return Ok(());
            }
            if table_exists(conn, LEGACY_FAVORITES_ARCHIVE_TABLE)? {
                return Ok(());
            }
            conn.execute_batch(&format!(
                "ALTER TABLE {LEGACY_FAVORITES_TABLE} RENAME TO {LEGACY_FAVORITES_ARCHIVE_TABLE};"
            ))?;
            tracing::info!(
                "Archived {LEGACY_FAVORITES_TABLE} → {LEGACY_FAVORITES_ARCHIVE_TABLE} after exporting favorites"
            );
            Ok(())
        })
    }
}

fn map_legacy_favorite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LegacyFavoriteRow> {
    let created_at: String = row.get(4)?;
    // A row with an unparseable timestamp must not abort the whole export —
    // that would strand every other favorite behind one bad row. It becomes
    // the epoch, which sorts last, and the id stays stable.
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| DateTime::<Utc>::from(std::time::UNIX_EPOCH));
    Ok(LegacyFavoriteRow {
        id: row.get(0)?,
        connection_id: row.get(1)?,
        title: row.get(2)?,
        sql: row.get(3)?,
        created_at,
    })
}

#[cfg(test)]
#[path = "legacy_favorites_tests.rs"]
mod legacy_favorites_tests;
