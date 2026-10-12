//! Owner-managed storage for reviewed Data Sync comparisons.
//!
//! The live merge engine writes differences directly to this store before the
//! result becomes part of an immutable plan. Compatibility callers may still
//! hand in a complete comparison; small values stay inline while larger values
//! use a private, process-local directory containing a manifest and indexed
//! framed row files. Page reads therefore seek directly to the requested table
//! and row range without deserializing the complete comparison.

use crate::data_sync::{ComparisonResult, DataSyncError, RowChange, RowChangeSink, TableResult};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
use datazen_driver_api::Value;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Comparisons at or below this serialized size remain inline.
pub(crate) const COMPARISON_MEMORY_LIMIT: usize = 8 * 1024 * 1024;
/// Full SQL/execute compatibility loads are intentionally bounded. Normal
/// review pages use the indexed path and do not require this limit.
pub(crate) const COMPARISON_FULL_LOAD_LIMIT: u64 = 64 * 1024 * 1024;

const DISK_FORMAT_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const ROW_FILE_PREFIX: &str = "table-";
const ROW_FILE_SUFFIX: &str = ".rows";
mod disk;
use disk::*;
mod manifest_cache;
use manifest_cache::ManifestCache;
mod recovery;
use recovery::*;

#[derive(Debug)]
enum Storage {
    Inline(ComparisonResult),
    File {
        root: PathBuf,
        directory: PathBuf,
        manifest: PathBuf,
        bytes: u64,
        owner: Arc<OwnerLease>,
    },
}

#[derive(Debug)]
struct Inner {
    storage: Storage,
    manifest_cache: ManifestCache,
    #[cfg(test)]
    full_load_calls: AtomicUsize,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Storage::File {
            root,
            directory,
            owner,
            ..
        } = &self.storage
        {
            remove_owned_store_directory(root, directory, owner.id);
        }
    }
}

/// Metadata required to render summaries without loading any row payloads.
#[derive(Debug, Clone)]
pub(crate) struct ComparisonTableMetadata {
    pub(crate) table: TableResult,
    pub(crate) row_count: usize,
    pub(crate) unchanged_count: usize,
    pub(crate) insert_count: usize,
    pub(crate) update_count: usize,
    pub(crate) delete_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowOffset {
    /// Offset of the JSON payload, immediately after the frame length prefix.
    offset: u64,
    length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskTable {
    /// `rows` is deliberately empty in the manifest. Row payloads live in the
    /// framed file named by `rows_file`.
    table: TableResult,
    rows_file: String,
    row_count: usize,
    unchanged_count: usize,
    insert_count: usize,
    update_count: usize,
    delete_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    index_file: Option<String>,
    #[serde(default)]
    offset_count: usize,
    offsets: Vec<RowOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiskManifest {
    format_version: u32,
    tables: Vec<DiskTable>,
}

/// Cloneable owner of a reviewed comparison. Clones share the temporary
/// directory owner and it is removed when the last plan/operation reference
/// drops.
#[derive(Clone, Debug)]
pub(crate) struct ComparisonStore {
    inner: Arc<Inner>,
}

/// Incremental writer used by the live comparison path. It owns one framed
/// row file per table and only keeps manifest metadata in memory. Dropping an
/// unfinished writer removes its private directory.
pub(crate) struct StreamingComparisonStoreWriter {
    root: PathBuf,
    owner: Arc<OwnerLease>,
    directory: Option<PathBuf>,
    tables: Vec<StreamingTable>,
    active: Option<usize>,
    failed: Option<String>,
    #[cfg(test)]
    write_failure: Option<TestWriteFailure>,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestWriteFailure {
    RowPayload,
    RowIndex,
    Manifest,
}

struct StreamingTable {
    table: TableResult,
    rows_file: String,
    index_file: String,
    file: File,
    index: File,
    row_count: usize,
    unchanged_count: usize,
    unchanged_row_count: usize,
    insert_count: usize,
    update_count: usize,
    delete_count: usize,
}

impl StreamingComparisonStoreWriter {
    pub(crate) fn new() -> Result<Self, String> {
        let root = ensure_private_store_root(&comparison_store_directory())
            .map_err(|error| format!("cannot prepare Data Sync comparison store root: {error}"))?;
        let owner = current_process_owner(&root)?;
        scavenge_stale_stores(&root);
        Self::new_with_owner(root, owner, None)
    }

    fn new_with_owner(
        root: PathBuf,
        owner: Arc<OwnerLease>,
        write_failure: Option<TestWriteFailure>,
    ) -> Result<Self, String> {
        let _ = &write_failure;
        let directory = create_store_directory(&root, &owner).map_err(|error| {
            format!("cannot create Data Sync comparison store directory: {error}")
        })?;
        Ok(Self {
            root,
            owner,
            directory: Some(directory),
            tables: Vec::new(),
            active: None,
            failed: None,
            #[cfg(test)]
            write_failure,
        })
    }

    #[cfg(test)]
    fn new_at(root: &Path, write_failure: Option<TestWriteFailure>) -> Result<Self, String> {
        let root = ensure_private_store_root(root)
            .map_err(|error| format!("cannot prepare Data Sync comparison store root: {error}"))?;
        let owner = acquire_owner_lease(&root)?;
        scavenge_stale_stores(&root);
        Self::new_with_owner(root, owner, write_failure)
    }

    fn abort_storage(&mut self, error: String) {
        self.failed = Some(error);
        self.active = None;
        // Close every row/index handle before cleanup. This is required for
        // Windows, where an open file handle can prevent directory removal.
        self.tables.clear();
        if let Some(directory) = self.directory.take() {
            remove_owned_store_directory(&self.root, &directory, self.owner.id);
        }
    }

    pub(crate) fn add_table(&mut self, mut table: TableResult) -> Result<(), String> {
        let unchanged_count = table.unchanged_count;
        let rows = std::mem::take(&mut table.rows);
        self.begin_table(table)?;
        for row in rows {
            self.write_row(row, true)
                .map_err(|error| error.to_string())?;
        }
        self.finish_table(unchanged_count)
    }

    pub(crate) fn begin_table(&mut self, mut table: TableResult) -> Result<(), String> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.active.is_some() {
            return Err("Data Sync comparison table is already being written".into());
        }
        let directory = self
            .directory
            .as_ref()
            .ok_or_else(|| "Data Sync comparison store writer is already finalized".to_string())?;
        let index = self.tables.len();
        let rows_file = format!("{ROW_FILE_PREFIX}{index}{ROW_FILE_SUFFIX}");
        let index_file = format!("{ROW_FILE_PREFIX}{index}.index");
        let files = (|| {
            let file = create_new_file(&directory.join(&rows_file))
                .map_err(|error| format!("cannot create Data Sync comparison row file: {error}"))?;
            let index_handle = create_new_file(&directory.join(&index_file)).map_err(|error| {
                format!("cannot create Data Sync comparison row index: {error}")
            })?;
            Ok::<_, String>((file, index_handle))
        })();
        let (file, index_handle) = match files {
            Ok(files) => files,
            Err(error) => {
                self.abort_storage(error.clone());
                return Err(error);
            }
        };
        table.rows.clear();
        table.unchanged_count = 0;
        self.tables.push(StreamingTable {
            table,
            rows_file,
            index_file,
            file,
            index: index_handle,
            row_count: 0,
            unchanged_count: 0,
            unchanged_row_count: 0,
            insert_count: 0,
            update_count: 0,
            delete_count: 0,
        });
        self.active = Some(index);
        Ok(())
    }

    pub(crate) fn finish_table(&mut self, unchanged_count: usize) -> Result<(), String> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        let result =
            (|| {
                let index = self
                    .active
                    .take()
                    .ok_or_else(|| "Data Sync comparison table is not being written".to_string())?;
                let table = self
                    .tables
                    .get_mut(index)
                    .ok_or_else(|| "Data Sync comparison table index is invalid".to_string())?;
                let total_unchanged_count = unchanged_count
                    .checked_add(table.unchanged_row_count)
                    .ok_or_else(|| "Data Sync unchanged row count overflowed".to_string())?;
                table.unchanged_count = total_unchanged_count;
                table.table.unchanged_count = unchanged_count;
                table.file.flush().map_err(|error| {
                    format!("cannot flush Data Sync comparison row file: {error}")
                })?;
                table.file.sync_all().map_err(|error| {
                    format!("cannot persist Data Sync comparison row file: {error}")
                })?;
                table.index.flush().map_err(|error| {
                    format!("cannot flush Data Sync comparison row index: {error}")
                })?;
                table.index.sync_all().map_err(|error| {
                    format!("cannot persist Data Sync comparison row index: {error}")
                })
            })();
        if let Err(error) = &result {
            self.abort_storage(error.clone());
        }
        result
    }

    pub(crate) fn finish(mut self) -> Result<ComparisonStore, String> {
        if let Some(error) = self.failed.take() {
            return Err(error);
        }
        if self.active.is_some() {
            return Err("Data Sync comparison table was not finalized".into());
        }
        let directory = match self.directory.as_ref() {
            Some(directory) => directory.clone(),
            None => return Err("Data Sync comparison store writer is already finalized".into()),
        };
        let result: Result<(PathBuf, u64), String> = (|| {
            let tables = self
                .tables
                .iter_mut()
                .map(|table| {
                    table.file.flush().map_err(|error| {
                        format!("cannot flush Data Sync comparison row file: {error}")
                    })?;
                    table.index.flush().map_err(|error| {
                        format!("cannot flush Data Sync comparison row index: {error}")
                    })?;
                    table.file.sync_all().map_err(|error| {
                        format!("cannot persist Data Sync comparison row file: {error}")
                    })?;
                    table.index.sync_all().map_err(|error| {
                        format!("cannot persist Data Sync comparison row index: {error}")
                    })?;
                    Ok(DiskTable {
                        table: table.table.clone(),
                        rows_file: table.rows_file.clone(),
                        row_count: table.row_count,
                        unchanged_count: table.unchanged_count,
                        insert_count: table.insert_count,
                        update_count: table.update_count,
                        delete_count: table.delete_count,
                        index_file: Some(table.index_file.clone()),
                        offset_count: table.row_count,
                        offsets: Vec::new(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            // The row files are immutable after this point; close them before
            // publishing the manifest or attempting cleanup on Windows.
            self.tables.clear();
            let manifest_path = directory.join(MANIFEST_FILE);
            let mut manifest_file = create_new_file(&manifest_path)
                .map_err(|error| format!("cannot create Data Sync comparison manifest: {error}"))?;
            let manifest = DiskManifest {
                format_version: DISK_FORMAT_VERSION,
                tables,
            };
            #[cfg(test)]
            if self.write_failure == Some(TestWriteFailure::Manifest) {
                let bytes = serde_json::to_vec(&manifest).map_err(|error| {
                    format!("cannot encode Data Sync comparison manifest: {error}")
                })?;
                let partial_len = (bytes.len() / 2).max(1).min(bytes.len());
                manifest_file
                    .write_all(&bytes[..partial_len])
                    .map_err(|error| {
                        format!("cannot write Data Sync comparison manifest: {error}")
                    })?;
                return Err(
                    "cannot write Data Sync comparison manifest: simulated disk failure".into(),
                );
            }
            serde_json::to_writer(&mut manifest_file, &manifest)
                .map_err(|error| format!("cannot write Data Sync comparison manifest: {error}"))?;
            manifest_file
                .flush()
                .map_err(|error| format!("cannot flush Data Sync comparison manifest: {error}"))?;
            manifest_file.sync_all().map_err(|error| {
                format!("cannot persist Data Sync comparison manifest: {error}")
            })?;
            let bytes = directory_bytes(&directory)?;
            Ok((manifest_path, bytes))
        })();
        match result {
            Ok((manifest, bytes)) => {
                let directory = match self.directory.take() {
                    Some(directory) => directory,
                    None => {
                        return Err("Data Sync comparison store writer is already finalized".into())
                    }
                };
                Ok(ComparisonStore {
                    inner: Arc::new(Inner {
                        manifest_cache: ManifestCache::default(),
                        storage: Storage::File {
                            root: self.root.clone(),
                            directory,
                            manifest,
                            bytes,
                            owner: self.owner.clone(),
                        },
                        #[cfg(test)]
                        full_load_calls: AtomicUsize::new(0),
                    }),
                })
            }
            Err(error) => {
                self.abort_storage(error.clone());
                Err(error)
            }
        }
    }

    fn push_row(&mut self, change: RowChange) -> Result<(), DataSyncError> {
        self.write_row(change, false)
    }

    /// Write one compatibility `TableResult` row, including unchanged row
    /// images. The live comparison sink continues to aggregate unchanged rows
    /// through `unchanged()` and therefore calls `push_row` with this disabled.
    fn write_row(
        &mut self,
        change: RowChange,
        preserve_unchanged: bool,
    ) -> Result<(), DataSyncError> {
        if let Some(error) = &self.failed {
            return Err(DataSyncError::validation(error.clone()));
        }
        if change.operation == crate::data_sync::ChangeOperation::Unchanged && !preserve_unchanged {
            let error =
                "unchanged rows must not be written to the Data Sync comparison index".to_string();
            self.abort_storage(error.clone());
            return Err(DataSyncError::validation(error));
        }
        let result: Result<(), String> = (|| {
            let index = self
                .active
                .ok_or_else(|| "Data Sync comparison row arrived outside a table".to_string())?;
            let table = self
                .tables
                .get_mut(index)
                .ok_or_else(|| "Data Sync comparison table index is invalid".to_string())?;
            let payload = serde_json::to_vec(&change)
                .map_err(|error| format!("cannot encode Data Sync comparison row: {error}"))?;
            let length = u64::try_from(payload.len())
                .map_err(|_| "Data Sync comparison row is too large".to_string())?;
            table
                .file
                .write_all(&length.to_le_bytes())
                .map_err(|error| format!("cannot write Data Sync comparison row frame: {error}"))?;
            let offset = table
                .file
                .stream_position()
                .map_err(|error| format!("cannot index Data Sync comparison row: {error}"))?;
            #[cfg(test)]
            if self.write_failure == Some(TestWriteFailure::RowPayload) {
                return Err("cannot write Data Sync comparison row: simulated disk failure".into());
            }
            table
                .file
                .write_all(&payload)
                .map_err(|error| format!("cannot write Data Sync comparison row: {error}"))?;
            #[cfg(test)]
            if self.write_failure == Some(TestWriteFailure::RowIndex) {
                table
                    .index
                    .write_all(&offset.to_le_bytes())
                    .map_err(|error| {
                        format!("cannot write Data Sync comparison row index: {error}")
                    })?;
                return Err(
                    "cannot write Data Sync comparison row index: simulated disk failure".into(),
                );
            }
            table
                .index
                .write_all(&offset.to_le_bytes())
                .and_then(|_| table.index.write_all(&length.to_le_bytes()))
                .map_err(|error| format!("cannot write Data Sync comparison row index: {error}"))?;
            table.row_count = table
                .row_count
                .checked_add(1)
                .ok_or_else(|| "Data Sync comparison row count overflowed".to_string())?;
            match change.operation {
                crate::data_sync::ChangeOperation::Insert => {
                    table.insert_count = table.insert_count.checked_add(1).ok_or_else(|| {
                        "Data Sync comparison insert count overflowed".to_string()
                    })?;
                }
                crate::data_sync::ChangeOperation::Update => {
                    table.update_count = table.update_count.checked_add(1).ok_or_else(|| {
                        "Data Sync comparison update count overflowed".to_string()
                    })?;
                }
                crate::data_sync::ChangeOperation::Delete => {
                    table.delete_count = table.delete_count.checked_add(1).ok_or_else(|| {
                        "Data Sync comparison delete count overflowed".to_string()
                    })?;
                }
                crate::data_sync::ChangeOperation::Unchanged => {
                    table.unchanged_row_count = table
                        .unchanged_row_count
                        .checked_add(1)
                        .ok_or_else(|| "Data Sync unchanged row count overflowed".to_string())?;
                }
            }
            Ok(())
        })();
        if let Err(error) = &result {
            self.abort_storage(error.clone());
        }
        result.map_err(DataSyncError::validation)
    }
}

impl Drop for StreamingComparisonStoreWriter {
    fn drop(&mut self) {
        self.tables.clear();
        if let Some(directory) = self.directory.take() {
            remove_owned_store_directory(&self.root, &directory, self.owner.id);
        }
    }
}

#[async_trait::async_trait]
impl RowChangeSink for StreamingComparisonStoreWriter {
    async fn push(&mut self, change: RowChange) -> Result<(), DataSyncError> {
        self.push_row(change)
    }

    async fn unchanged(&mut self) -> Result<(), DataSyncError> {
        if let Some(error) = &self.failed {
            return Err(DataSyncError::validation(error.clone()));
        }
        let result: Result<(), String> = (|| {
            let index = self
                .active
                .ok_or_else(|| "Data Sync unchanged row arrived outside a table".to_string())?;
            let table = self
                .tables
                .get_mut(index)
                .ok_or_else(|| "Data Sync comparison table index is invalid".to_string())?;
            table.unchanged_count = table
                .unchanged_count
                .checked_add(1)
                .ok_or_else(|| "Data Sync unchanged row count overflowed".to_string())?;
            Ok(())
        })();
        if let Err(error) = &result {
            self.abort_storage(error.clone());
        }
        result.map_err(DataSyncError::validation)
    }
}

impl ComparisonStore {
    pub(crate) fn from_comparison(comparison: ComparisonResult) -> Result<Self, String> {
        let mut probe = SizeProbe::new(COMPARISON_MEMORY_LIMIT);
        serde_json::to_writer(&mut probe, &comparison)
            .map_err(|error| format!("cannot serialize Data Sync comparison: {error}"))?;

        let storage = if probe.overflowed() {
            write_indexed_store(&comparison, probe.total())?
        } else {
            Storage::Inline(comparison)
        };
        Ok(Self {
            inner: Arc::new(Inner {
                manifest_cache: ManifestCache::default(),
                storage,
                #[cfg(test)]
                full_load_calls: AtomicUsize::new(0),
            }),
        })
    }

    /// Reload the complete comparison for SQL generation and execution. This
    /// is intentionally separate from `load_table_page`, whose page contract
    /// never calls this method.
    pub(crate) fn load(&self) -> Result<ComparisonResult, String> {
        #[cfg(test)]
        self.inner.full_load_calls.fetch_add(1, Ordering::Relaxed);

        match &self.inner.storage {
            Storage::Inline(comparison) => Ok(comparison.clone()),
            Storage::File {
                directory,
                manifest,
                bytes,
                ..
            } => {
                if *bytes > COMPARISON_FULL_LOAD_LIMIT {
                    return Err(format!(
                        "Data Sync comparison is too large for the full SQL/execute compatibility load ({} MiB); use paged review or select a smaller comparison",
                        COMPARISON_FULL_LOAD_LIMIT / (1024 * 1024)
                    ));
                }
                let manifest = self.inner.manifest_cache.read(manifest, directory)?;
                let mut tables = Vec::with_capacity(manifest.tables.len());
                for table in &manifest.tables {
                    let mut restored = table.table.clone();
                    restored.rows = read_all_rows(directory, table)?;
                    if restored.rows.len() != table.row_count {
                        return Err(format!(
                            "Data Sync comparison store row count mismatch for {} -> {}",
                            restored.source_table, restored.target_table
                        ));
                    }
                    tables.push(restored);
                }
                Ok(ComparisonResult::new(tables))
            }
        }
    }

    /// Return table metadata and counts without deserializing row payloads.
    pub(crate) fn summaries(&self) -> Result<Vec<ComparisonTableMetadata>, String> {
        match &self.inner.storage {
            Storage::Inline(comparison) => {
                Ok(comparison.tables.iter().map(metadata_from_table).collect())
            }
            Storage::File {
                directory,
                manifest,
                ..
            } => {
                let manifest = self.inner.manifest_cache.read(manifest, directory)?;
                Ok(manifest
                    .tables
                    .iter()
                    .map(metadata_from_disk_table)
                    .collect())
            }
        }
    }

    /// Read only one table's requested row range. The file-backed path opens
    /// exactly the selected table file and seeks to each indexed row frame.
    pub(crate) fn load_table_page(
        &self,
        source_table: &str,
        target_table: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<RowChange>, String> {
        if limit == 0 {
            return Err("comparison page limit must be greater than zero".into());
        }
        match &self.inner.storage {
            Storage::Inline(comparison) => {
                let table = comparison
                    .tables
                    .iter()
                    .find(|table| {
                        table.source_table == source_table && table.target_table == target_table
                    })
                    .ok_or_else(|| "table does not belong to the comparison plan".to_string())?;
                if offset > table.rows.len() {
                    return Err("comparison page offset is outside the comparison result".into());
                }
                let end = offset.saturating_add(limit).min(table.rows.len());
                Ok(table.rows[offset..end].to_vec())
            }
            Storage::File {
                directory,
                manifest,
                ..
            } => {
                let manifest = self.inner.manifest_cache.read(manifest, directory)?;
                let table = manifest
                    .tables
                    .iter()
                    .find(|table| {
                        table.table.source_table == source_table
                            && table.table.target_table == target_table
                    })
                    .ok_or_else(|| "table does not belong to the comparison plan".to_string())?;
                if offset > table.row_count {
                    return Err("comparison page offset is outside the comparison result".into());
                }
                let end = offset.saturating_add(limit).min(table.row_count);
                read_rows(directory, table, offset, end)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_spilled(&self) -> bool {
        matches!(self.inner.storage, Storage::File { .. })
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> Option<PathBuf> {
        match &self.inner.storage {
            Storage::Inline(_) => None,
            Storage::File { manifest, .. } => Some(manifest.clone()),
        }
    }

    #[cfg(test)]
    pub(crate) fn directory(&self) -> Option<PathBuf> {
        match &self.inner.storage {
            Storage::Inline(_) => None,
            Storage::File { directory, .. } => Some(directory.clone()),
        }
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        match &self.inner.storage {
            Storage::Inline(comparison) => serde_json::to_vec(comparison).unwrap().len(),
            Storage::File { bytes, .. } => *bytes as usize,
        }
    }

    #[cfg(test)]
    pub(crate) fn full_load_calls(&self) -> usize {
        self.inner.full_load_calls.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
#[path = "comparison_store/tests.rs"]
mod tests;
