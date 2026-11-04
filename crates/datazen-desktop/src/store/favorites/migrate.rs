//! One-time migration of `favorite_queries` rows into `.sql` files.
//!
//! Favorites used to live in `{appData}/history.sqlite`. Moving them to files
//! (plan §2.6) must not lose a single row, and the migration has to be safe to
//! interrupt: a laptop that sleeps mid-export, or a favorites root on a
//! read-only volume, must be recoverable without user action.
//!
//! ## Ordering
//!
//! 0. If `favorite_queries_legacy_v1` already exists, this database has run
//!    the export: stop. See [`migrate_legacy_favorites`] for why a live
//!    `favorite_queries` next to the archive must not trigger a re-export.
//! 1. Read every legacy row.
//! 2. Write one file per row, with a **deterministic** file name derived from
//!    the row itself.
//! 3. Only once every file is on disk, rename `favorite_queries` to its archive
//!    name and write the marker.
//!
//! Step 2 before step 3 is the whole safety argument. If the export dies half
//! way, the legacy table is untouched, so the next launch retries — and because
//! the names are derived from the row rather than freshly minted, a retry
//! rewrites the same paths instead of creating a second copy of everything.
//!
//! The archive (not a `DROP`) is the rollback: the original rows stay in the
//! original database, so the migration can be undone by renaming the table
//! back. `favorite_queries` as a *live* table is what retires here; the bytes
//! it held are not thrown away.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::ulid::Ulid;
use super::{FavoriteQuery, FavoritesRepository};
use crate::store::history_db::HistoryDb;

/// Marker file recording a completed export. Its presence is informational;
/// correctness comes from the archive rename, so a marker lost to a sync
/// conflict can never cause a double export.
const MARKER_FILE: &str = ".migration-v1.json";

/// Bumped only if a future export needs to run again. Must match the archive
/// table suffix below, so the two always move together.
const MIGRATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMarker {
    pub version: u32,
    pub migrated_at: DateTime<Utc>,
    /// Rows exported in the run that wrote this marker.
    pub count: usize,
    /// Where the pre-migration rows are kept, for rollback.
    pub archive_table: String,
    pub root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// No legacy table: a fresh install, or a store that already migrated.
    NothingToDo,
    /// Rows were exported and the legacy table archived.
    Migrated { rows: usize },
}

/// Export any remaining `favorite_queries` rows into `favorites`.
pub fn migrate_legacy_favorites(
    history: &HistoryDb,
    favorites: &FavoritesRepository,
) -> Result<MigrationOutcome, super::FavoritesError> {
    // The guard is the archive rename, checked *before* any row is read.
    //
    // A database that holds the archive has already run this export. If a live
    // `favorite_queries` sits next to it — a backup restore, a rolled-back
    // install, a hand-copied file — those rows are a resurrected copy, not
    // pre-migration data, and re-exporting them would rewrite the user's
    // current favorites on every single launch. The ids are deterministic, so
    // the re-export would not duplicate: it would silently overwrite whatever
    // the user has edited or deleted since, forever.
    if history.legacy_favorites_archived().map_err(|e| {
        super::FavoritesError::Migration(format!("cannot read the favorites archive state: {e}"))
    })? {
        return Ok(MigrationOutcome::NothingToDo);
    }

    let legacy = history.read_legacy_favorites().map_err(|e| {
        super::FavoritesError::Migration(format!("cannot read the legacy table: {e}"))
    })?;
    let Some(rows) = legacy else {
        // Either a fresh install (no table) or a store that already ran this
        // migration and renamed the table away. Both are a no-op, and neither
        // is worth a marker on disk.
        return Ok(MigrationOutcome::NothingToDo);
    };
    if rows.is_empty() {
        // A user who created and deleted every favorite still has the table.
        // There is nothing to export, so leave the schema exactly as found —
        // renaming it would be churn on a table the next reinstall drops.
        return Ok(MigrationOutcome::NothingToDo);
    }

    let mut written = 0usize;
    for row in &rows {
        let favorite = row.to_favorite();
        // A failure here propagates: the table is still intact, so the next
        // launch re-runs this loop and rewrites the same paths.
        favorites.import(&favorite).map_err(|e| {
            super::FavoritesError::Migration(format!("cannot export favorite {}: {e}", favorite.id))
        })?;
        written += 1;
    }

    history
        .archive_legacy_favorites_table()
        .map_err(|e| super::FavoritesError::Migration(e.to_string()))?;
    write_marker(&favorites.root(), written)
        .map_err(|e| super::FavoritesError::Migration(e.to_string()))?;

    tracing::info!(
        rows = written,
        root = %favorites.root().display(),
        "Migrated favorite_queries into the favorites directory"
    );
    Ok(MigrationOutcome::Migrated { rows: written })
}

fn marker_path(root: &Path) -> PathBuf {
    root.join(MARKER_FILE)
}

fn write_marker(root: &Path, count: usize) -> std::io::Result<()> {
    let marker = MigrationMarker {
        version: MIGRATION_VERSION,
        migrated_at: Utc::now(),
        count,
        archive_table: crate::store::history_db::LEGACY_FAVORITES_ARCHIVE_TABLE.to_string(),
        root: root.display().to_string(),
    };
    let json = serde_json::to_string_pretty(&marker)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    super::write_atomic(&marker_path(root), json.as_bytes())
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Read the marker written by a previous export, if any.
///
/// The marker exists for the user and for `排查`; nothing in the app reads it
/// back, because the archive table — not this file — is what makes the export
/// idempotent (see [`migrate_legacy_favorites`]). It is therefore test-only.
#[cfg(test)]
pub fn read_marker(root: &Path) -> Option<MigrationMarker> {
    let raw = std::fs::read_to_string(marker_path(root)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// A row of the retired `favorite_queries` table.
#[derive(Debug, Clone)]
pub struct LegacyFavoriteRow {
    pub id: String,
    pub connection_id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
}

impl LegacyFavoriteRow {
    /// Convert to the file-backed model, deriving a **deterministic** id.
    ///
    /// The id must be a pure function of the row: a retried export has to land
    /// on the same file names, or an interrupted migration would duplicate
    /// every favorite it had already written. The 48-bit timestamp comes from
    /// the row's own `created_at` (so exported files still sort by creation),
    /// and the 80-bit randomness from a digest of the legacy id.
    pub fn to_favorite(&self) -> FavoriteQuery {
        let id = deterministic_ulid(&self.id, self.created_at);
        FavoriteQuery {
            id,
            connection_id: self.connection_id.clone(),
            title: self.title.clone(),
            sql: self.sql.clone(),
            created_at: self.created_at,
            updated_at: None,
            keyword: None,
            database: None,
            folder: None,
        }
    }
}

/// Stable ULID for a migrated row. Same inputs ⇒ same file name, always.
fn deterministic_ulid(legacy_id: &str, created_at: DateTime<Utc>) -> String {
    let digest = Sha256::digest(legacy_id.as_bytes());
    let mut randomness = [0u8; 10];
    randomness.copy_from_slice(&digest[0..10]);
    let millis = u64::try_from(created_at.timestamp_millis()).unwrap_or(0);
    // A row timestamped before 1970 or absurdly far ahead still has to produce
    // a parseable, sortable id rather than an unwritable name.
    let millis = millis.min((1u64 << 48) - 1);
    match Ulid::from_parts(millis, randomness) {
        Ok(ulid) => ulid.to_string(),
        Err(_) => Ulid::from_parts(0, randomness)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| "0000000000000000000000000".to_string()),
    }
}
