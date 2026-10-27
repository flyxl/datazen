//! Disk-format serialization, validation, and indexed row access.

use super::*;
use serde_json::Deserializer;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub(super) fn metadata_from_table(table: &TableResult) -> ComparisonTableMetadata {
    ComparisonTableMetadata {
        table: TableResult {
            rows: Vec::new(),
            ..table.clone()
        },
        row_count: table.rows.len(),
        unchanged_count: table.unchanged_row_count(),
        insert_count: table.insert_count(),
        update_count: table.update_count(),
        delete_count: table.delete_count(),
    }
}

pub(super) fn metadata_from_disk_table(table: &DiskTable) -> ComparisonTableMetadata {
    ComparisonTableMetadata {
        table: table.table.clone(),
        row_count: table.row_count,
        unchanged_count: table.unchanged_count,
        insert_count: table.insert_count,
        update_count: table.update_count,
        delete_count: table.delete_count,
    }
}

fn disk_table_from_table(index: usize, table: &TableResult, offsets: Vec<RowOffset>) -> DiskTable {
    let metadata = metadata_from_table(table);
    let rows_file = format!("{ROW_FILE_PREFIX}{index}{ROW_FILE_SUFFIX}");
    DiskTable {
        table: metadata.table,
        rows_file,
        row_count: metadata.row_count,
        unchanged_count: metadata.unchanged_count,
        insert_count: metadata.insert_count,
        update_count: metadata.update_count,
        delete_count: metadata.delete_count,
        index_file: None,
        offset_count: offsets.len(),
        offsets,
    }
}

pub(super) fn write_indexed_store(
    comparison: &ComparisonResult,
    serialized_size: usize,
    store_root: &Path,
) -> Result<Storage, String> {
    let root = ensure_private_store_root(store_root)
        .map_err(|error| format!("cannot prepare Data Sync comparison store root: {error}"))?;
    let owner = current_process_owner(&root)?;
    scavenge_stale_stores(&root);
    let directory = create_store_directory(&root, &owner)
        .map_err(|error| format!("cannot create Data Sync comparison store directory: {error}"))?;
    let result = (|| {
        let mut tables = Vec::with_capacity(comparison.tables.len());
        for (index, table) in comparison.tables.iter().enumerate() {
            let rows_file = format!("{ROW_FILE_PREFIX}{index}{ROW_FILE_SUFFIX}");
            let path = directory.join(&rows_file);
            let mut file = create_new_file(&path)
                .map_err(|error| format!("cannot create Data Sync comparison row file: {error}"))?;
            let mut offsets = Vec::with_capacity(table.rows.len());
            for row in &table.rows {
                let payload = serde_json::to_vec(row).map_err(|error| {
                    format!("cannot serialize Data Sync comparison row: {error}")
                })?;
                let length = u64::try_from(payload.len())
                    .map_err(|_| "Data Sync comparison row is too large".to_string())?;
                file.write_all(&length.to_le_bytes())
                    .map_err(|error| format!("cannot write Data Sync comparison row: {error}"))?;
                let offset = file
                    .stream_position()
                    .map_err(|error| format!("cannot index Data Sync comparison row: {error}"))?;
                file.write_all(&payload)
                    .map_err(|error| format!("cannot write Data Sync comparison row: {error}"))?;
                offsets.push(RowOffset { offset, length });
            }
            file.flush()
                .map_err(|error| format!("cannot flush Data Sync comparison row file: {error}"))?;
            file.sync_all().map_err(|error| {
                format!("cannot persist Data Sync comparison row file: {error}")
            })?;
            tables.push(disk_table_from_table(index, table, offsets));
        }

        let manifest_path = directory.join(MANIFEST_FILE);
        let mut manifest_file = create_new_file(&manifest_path)
            .map_err(|error| format!("cannot create Data Sync comparison manifest: {error}"))?;
        let manifest = DiskManifest {
            format_version: DISK_FORMAT_VERSION,
            tables,
        };
        serde_json::to_writer(&mut manifest_file, &manifest)
            .map_err(|error| format!("cannot write Data Sync comparison manifest: {error}"))?;
        manifest_file
            .flush()
            .map_err(|error| format!("cannot flush Data Sync comparison manifest: {error}"))?;
        manifest_file
            .sync_all()
            .map_err(|error| format!("cannot persist Data Sync comparison manifest: {error}"))?;
        let bytes = directory_bytes(&directory)?;
        Ok(Storage::File {
            root: root.clone(),
            directory: directory.clone(),
            manifest: manifest_path,
            bytes: bytes.max(serialized_size as u64),
            owner: owner.clone(),
        })
    })();
    if result.is_err() {
        remove_owned_store_directory(&root, &directory, owner.id);
    }
    result
}

pub(super) fn directory_bytes(directory: &Path) -> Result<u64, String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot inspect Data Sync comparison store: {error}"))?;
    let mut total = 0u64;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot inspect Data Sync comparison store: {error}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("cannot inspect Data Sync comparison store: {error}"))?;
        total = total.saturating_add(metadata.len());
    }
    Ok(total)
}

pub(super) fn read_manifest(path: &Path, directory: &Path) -> Result<DiskManifest, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open Data Sync comparison store manifest: {error}"))?;
    let mut deserializer = Deserializer::from_reader(file);
    let manifest = DiskManifest::deserialize(&mut deserializer)
        .map_err(|error| format!("cannot decode Data Sync comparison store manifest: {error}"))?;
    deserializer.end().map_err(|error| {
        format!("Data Sync comparison store manifest contains trailing data: {error}")
    })?;
    validate_manifest(&manifest, directory)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &DiskManifest, directory: &Path) -> Result<(), String> {
    if manifest.format_version != DISK_FORMAT_VERSION {
        return Err("unsupported Data Sync comparison store format".into());
    }
    let mut identities = std::collections::HashSet::new();
    for table in &manifest.tables {
        if !table.table.rows.is_empty() {
            return Err("Data Sync comparison manifest unexpectedly contains row payloads".into());
        }
        if !identities.insert((
            table.table.source_table.clone(),
            table.table.target_table.clone(),
        )) {
            return Err("Data Sync comparison manifest contains duplicate tables".into());
        }
        let offset_count = table
            .index_file
            .as_ref()
            .map(|_| table.offset_count)
            .unwrap_or(table.offsets.len());
        if table.row_count != offset_count {
            return Err("Data Sync comparison manifest row index is inconsistent".into());
        }
        let file_name = Path::new(&table.rows_file);
        if file_name.components().count() != 1
            || file_name.file_name().and_then(|name| name.to_str())
                != Some(table.rows_file.as_str())
            || !table.rows_file.starts_with(ROW_FILE_PREFIX)
            || !table.rows_file.ends_with(ROW_FILE_SUFFIX)
        {
            return Err("Data Sync comparison manifest contains an invalid row file path".into());
        }
        let path = directory.join(&table.rows_file);
        let file_len = fs::metadata(&path)
            .map_err(|error| format!("cannot inspect Data Sync comparison row file: {error}"))?
            .len();
        if let Some(index_file) = &table.index_file {
            let index_name = Path::new(index_file);
            if index_name.components().count() != 1
                || index_name.file_name().and_then(|name| name.to_str()) != Some(index_file)
                || !index_file.starts_with(ROW_FILE_PREFIX)
                || !index_file.ends_with(".index")
            {
                return Err(
                    "Data Sync comparison manifest contains an invalid row index path".into(),
                );
            }
            validate_index_file(directory, index_file, table.row_count, &path, file_len)?;
        } else {
            let mut expected_end = 0u64;
            for row in &table.offsets {
                if row.offset < 8 || row.offset.checked_sub(8) != Some(expected_end) {
                    return Err("Data Sync comparison manifest contains invalid row offsets".into());
                }
                let end = row
                    .offset
                    .checked_add(row.length)
                    .ok_or_else(|| "Data Sync comparison row offset overflows".to_string())?;
                expected_end = end;
            }
            if file_len != expected_end {
                return Err("Data Sync comparison row file length does not match its index".into());
            }
            validate_frame_lengths(&path, &table.offsets)?;
        }
        validate_operation_counts(directory, table)?;
    }
    Ok(())
}

/// Recompute the mutating operation counters from the framed rows while
/// validating the manifest. This reads one payload at a time, so summary and
/// page validation stay bounded in memory while refusing a manifest whose
/// counters were changed independently of its row files.
fn validate_operation_counts(directory: &Path, table: &DiskTable) -> Result<(), String> {
    let path = directory.join(&table.rows_file);
    let mut rows = File::open(&path)
        .map_err(|error| format!("cannot open Data Sync comparison row file: {error}"))?;
    let mut index = table
        .index_file
        .as_ref()
        .map(|name| {
            File::open(directory.join(name))
                .map_err(|error| format!("cannot open Data Sync comparison row index: {error}"))
        })
        .transpose()?;
    let mut insert_count = 0usize;
    let mut update_count = 0usize;
    let mut delete_count = 0usize;
    let mut unchanged_row_count = 0usize;
    for position in 0..table.row_count {
        let offset = if let Some(index) = index.as_mut() {
            read_offset(index)?
        } else {
            table.offsets.get(position).cloned().ok_or_else(|| {
                "Data Sync comparison manifest row index is incomplete".to_string()
            })?
        };
        let frame_prefix = offset
            .offset
            .checked_sub(8)
            .ok_or_else(|| "Data Sync comparison row offset is invalid".to_string())?;
        rows.seek(SeekFrom::Start(frame_prefix))
            .map_err(|error| format!("cannot seek Data Sync comparison row frame: {error}"))?;
        let mut length_bytes = [0u8; 8];
        rows.read_exact(&mut length_bytes)
            .map_err(|error| format!("cannot read Data Sync comparison row frame: {error}"))?;
        if u64::from_le_bytes(length_bytes) != offset.length {
            return Err("Data Sync comparison row frame length does not match its index".into());
        }
        let length = usize::try_from(offset.length)
            .map_err(|_| "Data Sync comparison row is too large to read".to_string())?;
        let mut payload = vec![0u8; length];
        rows.read_exact(&mut payload)
            .map_err(|error| format!("cannot read Data Sync comparison row: {error}"))?;
        let change: RowChange = serde_json::from_slice(&payload)
            .map_err(|error| format!("cannot decode Data Sync comparison row: {error}"))?;
        match change.operation {
            crate::migration::data_sync::ChangeOperation::Insert => insert_count += 1,
            crate::migration::data_sync::ChangeOperation::Update => update_count += 1,
            crate::migration::data_sync::ChangeOperation::Delete => delete_count += 1,
            crate::migration::data_sync::ChangeOperation::Unchanged => {
                unchanged_row_count = unchanged_row_count
                    .checked_add(1)
                    .ok_or_else(|| "Data Sync unchanged row count overflowed".to_string())?;
            }
        }
    }
    let expected_unchanged_count = table
        .table
        .unchanged_count
        .checked_add(unchanged_row_count)
        .ok_or_else(|| "Data Sync unchanged row count overflowed".to_string())?;
    if insert_count != table.insert_count
        || update_count != table.update_count
        || delete_count != table.delete_count
        || expected_unchanged_count != table.unchanged_count
    {
        return Err("Data Sync comparison manifest operation counts do not match its rows".into());
    }
    Ok(())
}

/// Validate the fixed-size frame prefix for every indexed row without reading
/// or deserializing any row payload. This is part of manifest validation so
/// summaries fail closed on the same corruption that would reject a page or
/// full load.
fn validate_frame_lengths(path: &Path, offsets: &[RowOffset]) -> Result<(), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("cannot open Data Sync comparison row file: {error}"))?;
    for row in offsets {
        let frame_prefix = row
            .offset
            .checked_sub(8)
            .ok_or_else(|| "Data Sync comparison row offset is invalid".to_string())?;
        file.seek(SeekFrom::Start(frame_prefix))
            .map_err(|error| format!("cannot seek Data Sync comparison row frame: {error}"))?;
        let mut length_bytes = [0u8; 8];
        file.read_exact(&mut length_bytes)
            .map_err(|error| format!("cannot read Data Sync comparison row frame: {error}"))?;
        if u64::from_le_bytes(length_bytes) != row.length {
            return Err("Data Sync comparison row frame length does not match its index".into());
        }
    }
    Ok(())
}

fn validate_index_file(
    directory: &Path,
    index_name: &str,
    row_count: usize,
    rows_path: &Path,
    rows_len: u64,
) -> Result<(), String> {
    let index_path = directory.join(index_name);
    let index_len = fs::metadata(&index_path)
        .map_err(|error| format!("cannot inspect Data Sync comparison row index: {error}"))?
        .len();
    let expected_index_len = u64::try_from(row_count)
        .ok()
        .and_then(|count| count.checked_mul(16))
        .ok_or_else(|| "Data Sync comparison row index is too large".to_string())?;
    if index_len != expected_index_len {
        return Err("Data Sync comparison row index length does not match its count".into());
    }
    let mut index = File::open(&index_path)
        .map_err(|error| format!("cannot open Data Sync comparison row index: {error}"))?;
    let mut rows = File::open(rows_path)
        .map_err(|error| format!("cannot open Data Sync comparison row file: {error}"))?;
    let mut expected_end = 0u64;
    for _ in 0..row_count {
        let row = read_offset(&mut index)?;
        if row.offset < 8 || row.offset.checked_sub(8) != Some(expected_end) {
            return Err("Data Sync comparison manifest contains invalid row offsets".into());
        }
        rows.seek(SeekFrom::Start(row.offset - 8))
            .map_err(|error| format!("cannot seek Data Sync comparison row frame: {error}"))?;
        let mut length_bytes = [0u8; 8];
        rows.read_exact(&mut length_bytes)
            .map_err(|error| format!("cannot read Data Sync comparison row frame: {error}"))?;
        if u64::from_le_bytes(length_bytes) != row.length {
            return Err("Data Sync comparison row frame length does not match its index".into());
        }
        expected_end = row
            .offset
            .checked_add(row.length)
            .ok_or_else(|| "Data Sync comparison row offset overflows".to_string())?;
    }
    if rows_len != expected_end {
        return Err("Data Sync comparison row file length does not match its index".into());
    }
    Ok(())
}

fn read_offset(file: &mut File) -> Result<RowOffset, String> {
    let mut bytes = [0u8; 16];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("cannot read Data Sync comparison row index: {error}"))?;
    Ok(RowOffset {
        offset: u64::from_le_bytes(
            bytes[..8]
                .try_into()
                .map_err(|_| "invalid offset".to_string())?,
        ),
        length: u64::from_le_bytes(
            bytes[8..]
                .try_into()
                .map_err(|_| "invalid length".to_string())?,
        ),
    })
}

fn read_offset_range(
    directory: &Path,
    table: &DiskTable,
    start: usize,
    end: usize,
) -> Result<Vec<RowOffset>, String> {
    if let Some(index_name) = &table.index_file {
        let mut file = File::open(directory.join(index_name))
            .map_err(|error| format!("cannot open Data Sync comparison row index: {error}"))?;
        let start_bytes = u64::try_from(start)
            .ok()
            .and_then(|value| value.checked_mul(16))
            .ok_or_else(|| "Data Sync comparison row index offset overflows".to_string())?;
        file.seek(SeekFrom::Start(start_bytes))
            .map_err(|error| format!("cannot seek Data Sync comparison row index: {error}"))?;
        (start..end).map(|_| read_offset(&mut file)).collect()
    } else {
        Ok(table.offsets[start..end].to_vec())
    }
}

pub(super) fn read_all_rows(directory: &Path, table: &DiskTable) -> Result<Vec<RowChange>, String> {
    read_rows(directory, table, 0, table.row_count)
}

pub(super) fn read_rows(
    directory: &Path,
    table: &DiskTable,
    start: usize,
    end: usize,
) -> Result<Vec<RowChange>, String> {
    if start > end || end > table.row_count {
        return Err("Data Sync comparison page range is outside the indexed table".into());
    }
    let path = directory.join(&table.rows_file);
    let mut file = File::open(&path)
        .map_err(|error| format!("cannot open Data Sync comparison row file: {error}"))?;
    let offsets = read_offset_range(directory, table, start, end)?;
    let mut rows = Vec::with_capacity(end.saturating_sub(start));
    for row in &offsets {
        let frame_prefix = row
            .offset
            .checked_sub(8)
            .ok_or_else(|| "Data Sync comparison row offset is invalid".to_string())?;
        file.seek(SeekFrom::Start(frame_prefix))
            .map_err(|error| format!("cannot seek Data Sync comparison row: {error}"))?;
        let mut length_bytes = [0u8; 8];
        file.read_exact(&mut length_bytes)
            .map_err(|error| format!("cannot read Data Sync comparison row frame: {error}"))?;
        let framed_length = u64::from_le_bytes(length_bytes);
        if framed_length != row.length {
            return Err("Data Sync comparison row frame length does not match its index".into());
        }
        let length = usize::try_from(row.length)
            .map_err(|_| "Data Sync comparison row is too large to read".to_string())?;
        let mut payload = vec![0u8; length];
        file.read_exact(&mut payload)
            .map_err(|error| format!("cannot read Data Sync comparison row: {error}"))?;
        let row = serde_json::from_slice(&payload)
            .map_err(|error| format!("cannot decode Data Sync comparison row: {error}"))?;
        rows.push(row);
    }
    Ok(rows)
}

pub(super) struct SizeProbe {
    memory: Vec<u8>,
    limit: usize,
    total: usize,
}

impl SizeProbe {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            memory: Vec::new(),
            limit,
            total: 0,
        }
    }

    pub(super) fn overflowed(&self) -> bool {
        self.total > self.limit
    }

    pub(super) fn total(&self) -> usize {
        self.total
    }
}

impl Write for SizeProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.total = self.total.saturating_add(bytes.len());
        if self.memory.len() < self.limit {
            let remaining = self.limit - self.memory.len();
            self.memory
                .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn create_new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).read(true);
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path)
}
