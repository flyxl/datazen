//! Bounded local history for completed native workflow runs.

use datazen_ui_api::{WorkflowExecutionResult, WorkflowRunListItem, WorkflowRunRecord};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const KEEP_RUNS: usize = 100;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) struct FileWorkflowRunHistory {
    path: PathBuf,
    rows: Mutex<Result<Vec<WorkflowRunRecord>, String>>,
}

impl FileWorkflowRunHistory {
    pub(crate) fn open(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            rows: Mutex::new(Self::read(path)),
        }
    }

    fn read(path: &Path) -> Result<Vec<WorkflowRunRecord>, String> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Workflow history exceeds its storage limit.".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|_| "Could not read the local workflow history.".to_owned())
    }

    fn persist(&self, bytes: &[u8]) -> Result<(), String> {
        let parent = self.path.parent().ok_or("Workflow history has no parent directory.")?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        tmp.write_all(bytes).map_err(|error| error.to_string())?;
        tmp.as_file().sync_all().map_err(|error| error.to_string())?;
        tmp.persist(&self.path).map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn record(
        &self,
        workflow_id: String,
        workflow_name: String,
        result: WorkflowExecutionResult,
    ) -> Result<(), String> {
        let now = chrono::Local::now();
        let new = WorkflowRunRecord {
            id: datazen::workflow::history::new_run_id(now),
            workflow_id,
            workflow_name,
            // Do not persist the operator's raw parameter map. The report
            // is the existing engine output and is only opened on demand.
            variables: Default::default(),
            result,
            created_at: now.to_rfc3339(),
        };
        let mut guard = self.rows.lock().map_err(|error| error.to_string())?;
        let current = guard.as_mut().map_err(|error| error.clone())?;
        let newest = serde_json::to_vec(&new).map_err(|error| error.to_string())?;
        let mut total = newest.len() as u64 + 2;
        if total > MAX_BYTES {
            return Err("Workflow report exceeds its history storage limit.".into());
        }
        let mut encoded = vec![newest];
        for entry in current.iter().rev().take(KEEP_RUNS - 1) {
            let older = serde_json::to_vec(entry).map_err(|error| error.to_string())?;
            let size = older.len() as u64 + 1;
            if total + size > MAX_BYTES {
                break;
            }
            total += size;
            encoded.push(older);
        }
        // Preserve oldest-first storage; replay and clear observe the new data
        // only after the entire bounded document has been persisted.
        let mut bytes = Vec::with_capacity(total as usize);
        bytes.push(b'[');
        for (i, row) in encoded.iter().rev().enumerate() {
            if i > 0 {
                bytes.push(b',');
            }
            bytes.extend_from_slice(row);
        }
        bytes.push(b']');
        self.persist(&bytes)?;
        let retain = encoded.len() - 1;
        let remove = current.len().saturating_sub(retain);
        current.drain(..remove);
        current.push(new);
        Ok(())
    }

    pub(crate) fn list(&self) -> Result<Vec<WorkflowRunListItem>, String> {
        let guard = self.rows.lock().map_err(|error| error.to_string())?;
        let rows = guard.as_ref().map_err(Clone::clone)?;
        Ok(rows.iter().rev().map(|row| WorkflowRunListItem {
            id: row.id.clone(),
            workflow_id: row.workflow_id.clone(),
            workflow_name: row.workflow_name.clone(),
            success: row.result.success,
            total_time_ms: row.result.total_time_ms,
            created_at: row.created_at.clone(),
        }).collect())
    }

    pub(crate) fn get(&self, id: &str) -> Result<Option<WorkflowRunRecord>, String> {
        let guard = self.rows.lock().map_err(|error| error.to_string())?;
        let rows = guard.as_ref().map_err(Clone::clone)?;
        Ok(rows.iter().find(|row| row.id == id).cloned())
    }

    /// Remove only entries strictly older than a finite confirmed cutoff.
    pub(crate) fn retain_before(&self, cutoff: &str) -> Result<usize, String> {
        let cutoff = chrono::DateTime::parse_from_rfc3339(cutoff)
            .map_err(|_| "Invalid workflow history cutoff.".to_owned())?;
        if cutoff.with_timezone(&chrono::Utc) > chrono::Utc::now() - chrono::Duration::days(7) {
            return Err("Workflow retention must keep at least seven days.".into());
        }
        let mut guard = self.rows.lock().map_err(|error| error.to_string())?;
        let rows = guard.as_mut().map_err(|error| error.clone())?;
        let mut retained = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            let instant = chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map_err(|_| "Invalid timestamp in local workflow history.".to_owned())?;
            if instant >= cutoff {
                retained.push(row);
            }
        }
        let removed = rows.len() - retained.len();
        if removed != 0 {
            let bytes = serde_json::to_vec(&retained).map_err(|error| error.to_string())?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("Workflow history exceeds its storage limit.".into());
            }
            self.persist(&bytes)?;
            *rows = retained.into_iter().cloned().collect();
        }
        Ok(removed)
    }

    pub(crate) fn clear(&self) -> Result<(), String> {
        let mut guard = self.rows.lock().map_err(|error| error.to_string())?;
        let rows = guard.as_mut().map_err(|error| error.clone())?;
        self.persist(b"[]")?;
        rows.clear();
        Ok(())
    }
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    fn seed(path: &Path, times: &[&str]) -> Vec<u8> {
        let rows: Vec<_> = times.iter().enumerate().map(|(i, time)| serde_json::json!({
            "id": format!("fixture-{i}"),
            "workflowId": "retention-fixture",
            "workflowName": "Disposable retention fixture",
            "variables": {},
            "result": { "success": true, "finalOutput": "", "steps": [], "totalTimeMs": 1 },
            "createdAt": time,
        })).collect();
        let bytes = serde_json::to_vec_pretty(&rows).unwrap();
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    const CUTOFF: &str = "2020-04-01T00:00:00Z";
    const TIMES: &[&str] = &["2020-04-01T07:59:59+08:00", "2020-04-01T08:00:00+08:00", "2020-04-01T08:00:01+08:00"];

    #[test]
    fn finite_retention_keeps_the_exact_boundary_and_newer_runs_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow-run-history.json");
        seed(&path, TIMES);
        let store = FileWorkflowRunHistory::open(&path);
        assert_eq!(store.retain_before(CUTOFF).unwrap(), 1);
        for read in [store, FileWorkflowRunHistory::open(&path)] {
            let ids: Vec<_> = read.list().unwrap().into_iter().map(|row| row.id).collect();
            assert_eq!(ids, ["fixture-2", "fixture-1"]);
        }
    }

    #[test]
    fn finite_retention_does_not_publish_failed_or_invalid_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow-run-history.json");
        let corrupt = seed(&path, &[TIMES[0], "bad timestamp"]);
        let store = FileWorkflowRunHistory::open(&path);
        assert!(store.retain_before(CUTOFF).is_err());
        assert!(store.retain_before("2100-01-01T00:00:00Z").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);

        let original = seed(&path, TIMES);
        let store = FileWorkflowRunHistory::open(&path);
        let old = dir.path().join("unchanged-history.json");
        std::fs::rename(&path, &old).unwrap();
        // A directory at the publication path makes persist fail independent
        // of process permissions. Both the old disk bytes and cache survive.
        std::fs::create_dir(&path).unwrap();
        assert!(store.retain_before(CUTOFF).is_err());
        assert_eq!(std::fs::read(&old).unwrap(), original);
        assert_eq!(store.list().unwrap().len(), 3);
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(&old, &path).unwrap();
        assert_eq!(store.retain_before(CUTOFF).unwrap(), 1);
    }
}
