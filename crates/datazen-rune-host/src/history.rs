//! The host's query history (N9): one JSON file per store under the data
//! directory, written atomically, with the retention the repository contract
//! assigns to the storage side (the newest `KEEP` entries survive an add).

use async_trait::async_trait;
use datazen::history::{
    HistoryError, HistoryOrder, HistoryScope, MigrationRunFilter, MigrationRunPage,
    MigrationRunRecord, MigrationRunRepository, QueryHistoryEntry, QueryHistoryFilter,
    QueryHistoryPage, QueryHistoryRepository,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How many query entries the store keeps. The desktop keeps a bounded log;
/// a single JSON document keeps the same discipline.
const KEEP: usize = 1000;

fn io_error(error: impl std::fmt::Display) -> HistoryError {
    HistoryError::new(error.to_string())
}

fn parse_error(path: &Path, error: impl std::fmt::Display) -> HistoryError {
    HistoryError::new(format!("parse {}: {error}", path.display()))
}

/// The query history store: a JSON array of entries, newest last on disk.
pub struct FileQueryHistory {
    path: PathBuf,
    state: Mutex<Vec<QueryHistoryEntry>>,
}

impl FileQueryHistory {
    pub fn open(path: &Path) -> Result<Self, String> {
        let entries = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) if text.trim().is_empty() => Vec::new(),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self { path: path.to_owned(), state: Mutex::new(entries) })
    }

    fn persist(&self, entries: &[QueryHistoryEntry]) -> Result<(), HistoryError> {
        let text = serde_json::to_string_pretty(entries)
            .map_err(|error| HistoryError::new(format!("serialize the history: {error}")))?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(io_error)?;
        std::fs::rename(&temp, &self.path).map_err(io_error)?;
        Ok(())
    }
}

#[async_trait]
impl QueryHistoryRepository for FileQueryHistory {
    async fn add(&self, entry: QueryHistoryEntry) -> Result<(), HistoryError> {
        let mut entries = self.state.lock().expect("the history lock");
        // Retention is the storage side's job (the repository contract):
        // the newest KEEP entries survive.
        entries.push(entry);
        let overflow = entries.len().saturating_sub(KEEP);
        if overflow > 0 {
            entries.drain(..overflow);
        }
        self.persist(&entries)?;
        Ok(())
    }

    async fn recent(
        &self,
        limit: usize,
        connection_id: Option<&str>,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Result<Vec<QueryHistoryEntry>, HistoryError> {
        let entries = self.state.lock().expect("the history lock");
        let mut matched: Vec<QueryHistoryEntry> = entries
            .iter()
            .rev()
            .filter(|entry| {
                connection_id
                    .map(|id| entry.connection_id == id)
                    .unwrap_or(true)
                    && database.map(|db| entry.database == db).unwrap_or(true)
                    && schema
                        .map(|schema| entry.schema.as_deref() == Some(schema))
                        .unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect();
        matched.reverse(); // newest last, matching the desktop's reading order
        Ok(matched)
    }

    async fn page(
        &self,
        filter: &QueryHistoryFilter<'_>,
    ) -> Result<QueryHistoryPage, HistoryError> {
        let entries = self.state.lock().expect("the history lock");
        let mut matched: Vec<&QueryHistoryEntry> = entries
            .iter()
            .filter(|entry| {
                filter
                    .connection_id
                    .map(|id| entry.connection_id == id)
                    .unwrap_or(true)
                    && filter
                        .database
                        .map(|db| entry.database == db)
                        .unwrap_or(true)
                    && filter
                        .schema
                        .map(|schema| entry.schema.as_deref() == Some(schema))
                        .unwrap_or(true)
                    && filter
                        .search
                        .map(|needle| entry.sql.contains(needle))
                        .unwrap_or(true)
            })
            .collect();
        let total = matched.len() as u64;
        match filter.order {
            HistoryOrder::Recent => matched.reverse(),
            HistoryOrder::Oldest => {}
            HistoryOrder::Slowest => matched.sort_by_key(|entry| std::cmp::Reverse(entry.execution_time_ms)),
        }
        let selected = matched.into_iter().take(filter.limit).cloned().collect();
        Ok(QueryHistoryPage { entries: selected, total })
    }

    async fn delete(&self, id: &str) -> Result<u64, HistoryError> {
        let mut entries = self.state.lock().expect("the history lock");
        let before = entries.len();
        entries.retain(|entry| entry.id != id);
        let removed = (before - entries.len()) as u64;
        if removed > 0 {
            self.persist(&entries)?;
        }
        Ok(removed)
    }

    async fn purge(
        &self,
        scope: HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, HistoryError> {
        if !matches!(scope, HistoryScope::Query | HistoryScope::All) {
            return Ok(0); // the workflow domain is not this store's
        }
        let mut entries = self.state.lock().expect("the history lock");
        let before = entries.len();
        match retain_days {
            None => entries.clear(),
            Some(days) => {
                let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
                entries.retain(|entry| entry.executed_at > cutoff);
            }
        }
        let removed = (before - entries.len()) as u64;
        if removed > 0 {
            self.persist(&entries)?;
        }
        Ok(removed)
    }
}

/// The migration-run store (Data Sync / Transfer / Schema Diff runs, N12's
/// domain): a JSON array of records, newest last.
pub struct FileMigrationRuns {
    path: PathBuf,
    state: Mutex<Vec<MigrationRunRecord>>,
}

impl FileMigrationRuns {
    pub fn open(path: &Path) -> Result<Self, String> {
        let runs = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) if text.trim().is_empty() => Vec::new(),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self { path: path.to_owned(), state: Mutex::new(runs) })
    }

    fn persist(&self, runs: &[MigrationRunRecord]) -> Result<(), HistoryError> {
        let text = serde_json::to_string_pretty(runs)
            .map_err(|error| HistoryError::new(format!("serialize the runs: {error}")))?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(io_error)?;
        std::fs::rename(&temp, &self.path).map_err(io_error)?;
        Ok(())
    }
}

#[async_trait]
impl MigrationRunRepository for FileMigrationRuns {
    async fn save(&self, run: &MigrationRunRecord) -> Result<(), HistoryError> {
        let mut runs = self.state.lock().expect("the runs lock");
        match runs.iter_mut().find(|existing| existing.id == run.id) {
            Some(existing) => *existing = run.clone(),
            None => runs.push(run.clone()),
        }
        self.persist(&runs)
    }

    async fn get(&self, id: &str) -> Result<Option<MigrationRunRecord>, HistoryError> {
        let runs = self.state.lock().expect("the runs lock");
        Ok(runs.iter().find(|run| run.id == id).cloned())
    }

    async fn list(
        &self,
        filter: &MigrationRunFilter,
        offset: u64,
        limit: u64,
    ) -> Result<MigrationRunPage, HistoryError> {
        let runs = self.state.lock().expect("the runs lock");
        let matched: Vec<&MigrationRunRecord> = runs
            .iter()
            .filter(|run| {
                filter
                    .operation
                    .as_deref()
                    .map(|op| run.operation == op)
                    .unwrap_or(true)
                    && filter
                        .status
                        .as_deref()
                        .map(|status| run.status == status)
                        .unwrap_or(true)
                    && filter
                        .profile_id
                        .as_deref()
                        .map(|id| run.profile_id.as_deref() == Some(id))
                        .unwrap_or(true)
                    && filter
                        .connection_id
                        .as_deref()
                        .map(|id| {
                            run.source_connection_id.as_deref() == Some(id)
                                || run.target_connection_id.as_deref() == Some(id)
                        })
                        .unwrap_or(true)
            })
            .collect();
        let total = matched.len() as u64;
        let items = matched
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect();
        Ok(MigrationRunPage { items, total, offset, limit })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("query-history.json")
    }

    fn entry(id: &str, sql: &str, connection: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: id.into(),
            connection_id: connection.into(),
            database: "main".into(),
            schema: None,
            sql: sql.into(),
            executed_at: chrono::Utc::now(),
            execution_time_ms: 12,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        }
    }

    #[test]
    fn add_then_recent_reads_the_same_entry_back() {
        let path = temp_path("round-trip");
        let history = FileQueryHistory::open(&path).unwrap();
        futures_executor::block_on(async {
            history.add(entry("q1", "SELECT 1", "live")).await.unwrap();
        });

        // Through a fresh store over the same file: persistence, and the
        // narrow read answers the same entry.
        let history = FileQueryHistory::open(&path).unwrap();
        let recent =
            futures_executor::block_on(history.recent(10, Some("live"), Some("main"), None))
                .unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, "q1");
        assert_eq!(recent[0].sql, "SELECT 1");

        // The filter is a filter: another connection sees nothing.
        let other =
            futures_executor::block_on(history.recent(10, Some("other"), None, None)).unwrap();
        assert!(other.is_empty(), "the connection filter narrows");
    }

    #[test]
    fn retention_keeps_the_newest_entries() {
        let path = temp_path("retention");
        let history = FileQueryHistory::open(&path).unwrap();
        for index in 0..(KEEP + 10) {
            let mut tail = entry("", "SELECT 1", "live");
            tail.id = format!("q{index:05}");
            futures_executor::block_on(history.add(tail)).unwrap();
        }
        let runs = std::fs::read_to_string(&path).unwrap();
        let entries: Vec<QueryHistoryEntry> = serde_json::from_str(&runs).unwrap();
        assert_eq!(entries.len(), KEEP, "the store keeps exactly KEEP entries");
        assert_eq!(entries.first().unwrap().id, "q00010", "the oldest beyond KEEP dropped");
        assert_eq!(entries.last().unwrap().id, format!("q{:05}", KEEP + 9), "the newest kept");
    }

    #[test]
    fn migration_runs_save_get_and_list() {
        let dir = std::env::temp_dir().join(format!(
            "datazen-rune-history-runs-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A record in the sync domain's full wire shape — the store only
        // moves it, so the test builds it through serde.
        let run: MigrationRunRecord = serde_json::from_value(serde_json::json!({
            "id": "run-1", "operation": "data_sync", "status": "succeeded",
            "outcome": "applied", "phase": "done", "profileId": null,
            "profileRevision": null, "sourceConnectionId": "src", "targetConnectionId": "dst",
            "startedAt": "2026-01-01T00:00:00Z", "finishedAt": null,
            "selectedCount": 1, "committedCount": 1, "failedCount": 0,
            "conflictCount": 0, "cancelled": false, "rollbackOutcome": "none",
            "errorSummary": null
        }))
        .expect("the record deserializes from the wire shape");
        let store = FileMigrationRuns::open(&dir.join("runs.json")).unwrap();
        futures_executor::block_on(async {
            store.save(&run).await.unwrap();
        });
        let got = futures_executor::block_on(store.get("run-1")).unwrap().expect("the run is there");
        assert_eq!(got.operation, "data_sync");
        let listed = futures_executor::block_on(store.list(&MigrationRunFilter::default(), 0, 10))
            .unwrap();
        assert_eq!(listed.total, 1);
        // The connection filter narrows by either side of the run.
        let by_connection = futures_executor::block_on(store.list(
            &MigrationRunFilter {
                connection_id: Some("dst".into()),
                ..MigrationRunFilter::default()
            },
            0,
            10,
        ))
        .unwrap();
        assert_eq!(by_connection.total, 1, "the target side matches");
        let by_other = futures_executor::block_on(store.list(
            &MigrationRunFilter {
                connection_id: Some("other".into()),
                ..MigrationRunFilter::default()
            },
            0,
            10,
        ))
        .unwrap();
        assert_eq!(by_other.total, 0, "an unrelated connection matches nothing");
    }
}
