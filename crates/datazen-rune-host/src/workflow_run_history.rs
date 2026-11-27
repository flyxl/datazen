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

    pub(crate) fn clear(&self) -> Result<(), String> {
        let mut guard = self.rows.lock().map_err(|error| error.to_string())?;
        let rows = guard.as_mut().map_err(|error| error.clone())?;
        self.persist(b"[]")?;
        rows.clear();
        Ok(())
    }
}
