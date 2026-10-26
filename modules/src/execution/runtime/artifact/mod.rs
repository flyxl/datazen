//! Durable, bounded result storage. Metadata rename is the publication boundary.
//! Unfinished writers are recovered as truncated; writer capabilities never survive restart.
mod execution;
/// 已发布字节配额：定义在 `crate::execution::domain::artifact_repository`（契约值）。
pub use crate::execution::ARTIFACT_BYTE_QUOTA;

use async_trait::async_trait;
use crate::execution::domain::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ArtifactSummary,
    ResultCompleteness,
};
use crate::execution::domain::artifact_repository::*;
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::*;
pub(crate) use execution::ExecutionArtifactScope;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize, Clone)]
struct Record {
    organization: OrganizationId,
    owner: PrincipalId,
    execution: ExecutionId,
    #[serde(default)]
    delegated_execution: bool,
    #[serde(skip)]
    execution_scope: Option<ExecutionArtifactScope>,
    fingerprint: String,
    expires: u64,
    metadata: ArtifactMetadata,
    sizes: Vec<u64>,
    #[serde(skip)]
    writer: Option<LeaseId>,
    #[serde(skip)]
    unpublished_bytes: u64,
}
#[derive(Default)]
struct State {
    records: HashMap<ArtifactId, Record>,
    exports: HashMap<LeaseId, ExportGrant>,
    expired: HashMap<ArtifactId, (OrganizationId, PrincipalId, u64)>,
}
struct ExportGrant {
    path: PathBuf,
    organization: OrganizationId,
    owner: PrincipalId,
    expires: u64,
    allow_truncated: bool,
    artifact: Option<ArtifactId>,
    cancelled: Option<Arc<AtomicBool>>,
}
pub struct FileArtifactRepository {
    root: PathBuf,
    state: Mutex<State>,
}
fn unavailable() -> PortError {
    PortError::BackendUnavailable("artifact storage unavailable".into())
}
fn invalid() -> PortError {
    PortError::cas_conflict("artifact", "invalid writer or unpublished range")
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0)
}
#[cfg(unix)]
fn private_dir(path: &Path) -> Result<(), PortError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| unavailable())
}
#[cfg(not(unix))]
fn private_dir(_path: &Path) -> Result<(), PortError> {
    Ok(())
}
fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}
fn read_metadata(path: &Path) -> Option<Record> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 2 * 1024 * 1024 {
        return None;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    let file = private_file(&mut options).open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
fn read_block(path: &Path, size: u64) -> Result<Vec<u8>, PortError> {
    if size > 32 * 1024 * 1024 {
        return Err(unavailable());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    let file = private_file(&mut options)
        .open(path)
        .map_err(|_| unavailable())?;
    let meta = file.metadata().map_err(|_| unavailable())?;
    if !meta.is_file() || meta.len() != size {
        return Err(unavailable());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 != size {
        return Err(unavailable());
    }
    Ok(bytes)
}
impl FileArtifactRepository {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, PortError> {
        if fs::symlink_metadata(root.as_ref()).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(unavailable());
        }
        fs::create_dir_all(root.as_ref()).map_err(|_| unavailable())?;
        private_dir(root.as_ref())?;
        let root = fs::canonicalize(root.as_ref()).map_err(|_| unavailable())?;
        let store = Self {
            root,
            state: Mutex::new(State::default()),
        };
        let mut state = store.state.lock().map_err(|_| unavailable())?;
        for item in fs::read_dir(&store.root).map_err(|_| unavailable())? {
            let item = item.map_err(|_| unavailable())?;
            if !item.file_type().map_err(|_| unavailable())?.is_dir() {
                continue;
            }
            let file = item.path().join("metadata.json");
            if !file.exists() {
                fs::remove_dir_all(item.path()).map_err(|_| unavailable())?;
                continue;
            }
            private_dir(&item.path())?;
            let loaded = read_metadata(&file);
            let Some(mut record) = loaded else {
                fs::remove_dir_all(item.path()).map_err(|_| unavailable())?;
                tracing::warn!("corrupt artifact metadata removed during recovery");
                continue;
            };
            // Delegated result access is a process-local admitted capability.
            // Neither its scope nor writer may be reconstructed after restart.
            if record.delegated_execution {
                fs::remove_dir_all(item.path()).map_err(|_| unavailable())?;
                continue;
            }
            let valid = item.file_name().to_str() == Some(record.metadata.artifact_id.as_str())
                && record.metadata.published_chunk_count.get() == record.sizes.len() as u64
                && record.sizes.len() <= 65536
                && record.sizes.iter().all(|size| *size <= 32 * 1024 * 1024)
                && record
                    .sizes
                    .iter()
                    .try_fold(0u64, |sum, size| sum.checked_add(*size))
                    == Some(record.metadata.published_byte_size.get())
                && record.metadata.published_byte_size.get() <= ARTIFACT_BYTE_QUOTA
                && record.sizes.iter().enumerate().all(|(index, size)| {
                    fs::symlink_metadata(item.path().join(format!("{index}.chunk")))
                        .is_ok_and(|meta| meta.is_file() && meta.len() == *size)
                });
            let total = state.records.values().fold(0u64, |sum, r| {
                sum.saturating_add(r.metadata.published_byte_size.get())
                    .saturating_add(r.unpublished_bytes)
            });
            if !valid
                || state.records.len() >= 4096
                || total.saturating_add(record.metadata.published_byte_size.get())
                    > ARTIFACT_BYTE_QUOTA
            {
                fs::remove_dir_all(item.path()).map_err(|_| unavailable())?;
                tracing::warn!("invalid or excess artifact removed during recovery");
                continue;
            }
            if record.metadata.lifecycle == ArtifactLifecycle::Writing {
                record.metadata.lifecycle = ArtifactLifecycle::Truncated;
                record.metadata.result_completeness = ResultCompleteness::Truncated;
                record.metadata.truncation_reason = Some(AbortReason::WriterLost);
                record.metadata.total_chunks = Some(record.metadata.published_chunk_count);
                store.commit(&record)?;
            }
            // A crash before metadata commit can leave an unpublished chunk. Remove it;
            // it must never become visible or block a future storage reservation.
            for chunk in fs::read_dir(item.path()).map_err(|_| unavailable())? {
                let chunk = chunk.map_err(|_| unavailable())?;
                let name = chunk.file_name();
                let name = name.to_string_lossy();
                if let Some(index) = name
                    .strip_suffix(".chunk")
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    if index >= record.sizes.len() {
                        fs::remove_file(chunk.path()).map_err(|_| unavailable())?;
                    }
                }
            }
            state
                .records
                .insert(record.metadata.artifact_id.clone(), record);
        }
        drop(state);
        store.expire_at(now())?;
        Ok(store)
    }
    fn dir(&self, id: &ArtifactId) -> PathBuf {
        self.root.join(id.as_str())
    }
    fn commit(&self, record: &Record) -> Result<(), PortError> {
        let dir = self.dir(&record.metadata.artifact_id);
        let bytes = serde_json::to_vec(record).map_err(|_| unavailable())?;
        let temp = dir.join("metadata.pending");
        let mut file = private_file(OpenOptions::new().write(true).create(true).truncate(true))
            .open(&temp)
            .map_err(|_| unavailable())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| unavailable())?;
        fs::rename(temp, dir.join("metadata.json")).map_err(|_| unavailable())?;
        fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| unavailable())
    }
    fn visible<'a>(
        &self,
        state: &'a State,
        ctx: &RequestContext,
        id: &ArtifactId,
    ) -> Result<&'a Record, PortError> {
        if state
            .expired
            .get(id)
            .is_some_and(|(organization, owner, _)| {
                *organization == ctx.organization_id
                    && *owner == ctx.principal_id
                    && ctx.delegation_id.is_none()
            })
        {
            return Err(PortError::ArtifactExpired);
        }
        let r = state
            .records
            .get(id)
            .filter(|r| {
                !r.delegated_execution
                    && r.organization == ctx.organization_id
                    && r.owner == ctx.principal_id
                    && ctx.delegation_id.is_none()
            })
            .ok_or_else(|| PortError::NotFound("artifact".into()))?;
        if r.expires <= now() || r.metadata.lifecycle == ArtifactLifecycle::Expired {
            return Err(PortError::ArtifactExpired);
        }
        if r.metadata.lifecycle == ArtifactLifecycle::Revoked {
            return Err(PortError::NotFound("artifact".into()));
        }
        Ok(r)
    }
    fn writer<'a>(&self, state: &'a State, w: &ArtifactWriter) -> Result<&'a Record, PortError> {
        let r = state.records.get(&w.artifact_id).ok_or_else(invalid)?;
        if r.expires <= now() {
            return Err(PortError::ArtifactExpired);
        }
        if r.writer.as_ref() != Some(&w.lease_id)
            || r.metadata.lifecycle != ArtifactLifecycle::Writing
            || w.next_chunk_index != r.metadata.published_chunk_count
            || w.published_chunk_count != r.metadata.published_chunk_count
            || w.published_byte_size != r.metadata.published_byte_size
        {
            return Err(invalid());
        }
        Ok(r)
    }
    pub fn create_sync(
        &self,
        ctx: &RequestContext,
        spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        if spec.organization_id != ctx.organization_id
            || spec.owner_principal_id != ctx.principal_id
            || ctx.delegation_id.is_some()
            || spec.ttl_seconds == 0
        {
            return Err(PortError::TokenInvalid);
        }
        self.create_record(spec, None)
    }
    pub fn append_sync(
        &self,
        writer: &mut ArtifactWriter,
        chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError> {
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        let mut record = self.writer(&state, writer)?.clone();
        let total = state.records.values().fold(0u64, |sum, r| {
            sum.saturating_add(r.metadata.published_byte_size.get())
                .saturating_add(r.unpublished_bytes)
        });
        let size = chunk.bytes.len() as u64;
        if size > 32 * 1024 * 1024
            || total.saturating_add(size) > ARTIFACT_BYTE_QUOTA
            || record.sizes.len() >= 65536
        {
            return Err(PortError::QuotaExceeded("artifact byte budget".into()));
        }
        let index = writer.next_chunk_index;
        let path = self
            .dir(&writer.artifact_id)
            .join(format!("{}.chunk", index.get()));
        let mut file = private_file(OpenOptions::new().write(true).create_new(true))
            .open(&path)
            .map_err(|_| unavailable())?;
        if file
            .write_all(&chunk.bytes)
            .and_then(|_| file.sync_all())
            .is_err()
        {
            if fs::remove_file(&path).is_err() {
                record.unpublished_bytes = size;
                record.writer = None;
                record.metadata.lifecycle = ArtifactLifecycle::Truncated;
                record.metadata.result_completeness = ResultCompleteness::Truncated;
                record.metadata.truncation_reason = Some(AbortReason::WriterLost);
                record.metadata.total_chunks = Some(record.metadata.published_chunk_count);
                let _ = self.commit(&record);
                state.records.insert(writer.artifact_id.clone(), record);
            }
            return Err(unavailable());
        }
        record.sizes.push(size);
        record.metadata.published_chunk_count = index.saturating_increment();
        record.metadata.published_byte_size = record
            .metadata
            .published_byte_size
            .saturating_increment_by(size);
        if let Err(error) = self.commit(&record) {
            // rename may have committed before directory fsync failed. Preserve
            // exactly that committed prefix, never delete or overwrite its chunks.
            let committed = read_metadata(&self.dir(&writer.artifact_id).join("metadata.json"))
                .filter(|r| {
                    r.metadata.artifact_id == writer.artifact_id
                        && r.metadata.published_chunk_count == record.metadata.published_chunk_count
                });
            let mut interrupted = committed.unwrap_or_else(|| {
                let mut previous = record.clone();
                previous.sizes.pop();
                previous.metadata.published_chunk_count = writer.published_chunk_count;
                previous.metadata.published_byte_size = writer.published_byte_size;
                previous.unpublished_bytes = size;
                previous
            });
            interrupted.metadata.lifecycle = ArtifactLifecycle::Truncated;
            interrupted.metadata.result_completeness = ResultCompleteness::Truncated;
            interrupted.metadata.truncation_reason = Some(AbortReason::WriterLost);
            interrupted.metadata.total_chunks = Some(interrupted.metadata.published_chunk_count);
            interrupted.writer = None;
            let _ = self.commit(&interrupted);
            state
                .records
                .insert(writer.artifact_id.clone(), interrupted);
            return Err(error);
        }
        writer.next_chunk_index = record.metadata.published_chunk_count;
        writer.published_chunk_count = record.metadata.published_chunk_count;
        writer.published_byte_size = record.metadata.published_byte_size;
        state.records.insert(writer.artifact_id.clone(), record);
        Ok(index)
    }
    pub fn describe_sync(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
    ) -> Result<ArtifactMetadata, PortError> {
        let s = self.state.lock().map_err(|_| unavailable())?;
        let record = self.visible(&s, ctx, id)?;
        let mut metadata = record.metadata.clone();
        // The creation record is authoritative, including artifacts written by
        // older versions before this field was exposed by describe.
        metadata.target_fingerprint = record.fingerprint.clone();
        Ok(metadata)
    }
    pub fn finish_sync(
        &self,
        writer: ArtifactWriter,
        reason: Option<AbortReason>,
    ) -> Result<ArtifactId, PortError> {
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        let mut r = self.writer(&s, &writer)?.clone();
        r.metadata.lifecycle = if reason.is_some() {
            ArtifactLifecycle::Truncated
        } else {
            ArtifactLifecycle::Complete
        };
        r.metadata.result_completeness = if reason.is_some() {
            ResultCompleteness::Truncated
        } else {
            ResultCompleteness::Complete
        };
        r.metadata.truncation_reason = reason;
        r.metadata.total_chunks = Some(r.metadata.published_chunk_count);
        r.writer = None;
        if let Err(error) = self.commit(&r) {
            // Publication may already be durable; invalidate this writer and
            // expose a terminal uncertain truncation rather than retrying writes.
            r.metadata.lifecycle = ArtifactLifecycle::Truncated;
            r.metadata.result_completeness = ResultCompleteness::Truncated;
            r.metadata.truncation_reason = Some(AbortReason::WriterLost);
            let _ = self.commit(&r);
            s.records.insert(writer.artifact_id.clone(), r);
            return Err(error);
        }
        s.records.insert(writer.artifact_id.clone(), r);
        Ok(writer.artifact_id)
    }
    pub fn read_sync(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
        index: Counter,
    ) -> Result<ArtifactChunk, PortError> {
        let s = self.state.lock().map_err(|_| unavailable())?;
        let r = self.visible(&s, ctx, id)?;
        self.read_record(id, index, r)
    }
    pub fn reclaim_expired_sync(&self) -> Result<Vec<ArtifactId>, PortError> {
        self.expire_at(now())
    }
    fn expire_at(&self, at: u64) -> Result<Vec<ArtifactId>, PortError> {
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        let ids: Vec<_> = s
            .records
            .iter()
            .filter(|(_, r)| r.expires <= at)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            fs::remove_dir_all(self.dir(id)).map_err(|_| unavailable())?;
            if let Some(record) = s.records.remove(id) {
                if s.expired.len() >= 4096 {
                    if let Some(old) = s
                        .expired
                        .iter()
                        .min_by_key(|(_, (_, _, expires))| *expires)
                        .map(|(id, _)| id.clone())
                    {
                        s.expired.remove(&old);
                    }
                }
                s.expired.insert(
                    id.clone(),
                    (record.organization, record.owner, at.saturating_add(3600)),
                );
            }
        }
        s.expired.retain(|_, (_, _, expires)| *expires > at);
        Ok(ids)
    }
    /// Host capability registration after a successful save dialog. Never call with request paths.
    pub fn authorize_export(
        &self,
        ctx: &RequestContext,
        handle: LeaseId,
        path: PathBuf,
    ) -> Result<(), PortError> {
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        s.exports.retain(|_, grant| grant.expires > now());
        if ctx.delegation_id.is_some() {
            return Err(PortError::TokenInvalid);
        }
        if s.exports.len() >= 64 {
            return Err(PortError::QuotaExceeded("export handles".into()));
        }
        if s.exports.contains_key(&handle) {
            return Err(PortError::TokenInvalid);
        }
        s.exports.insert(
            handle,
            ExportGrant {
                path,
                organization: ctx.organization_id.clone(),
                owner: ctx.principal_id.clone(),
                expires: now().saturating_add(300),
                allow_truncated: false,
                artifact: None,
                cancelled: None,
            },
        );
        Ok(())
    }
    /// Register an artifact-specific native-dialog capability. Cancellation is host-owned.
    pub fn authorize_dialog_export(
        &self,
        ctx: &RequestContext,
        id: ArtifactId,
        handle: LeaseId,
        path: PathBuf,
        partial_confirmed: bool,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(), PortError> {
        self.authorize_export(ctx, handle.clone(), path)?;
        let mut state = self.state.lock().map_err(|_| unavailable())?;
        let grant = state
            .exports
            .get_mut(&handle)
            .ok_or(PortError::TokenInvalid)?;
        grant.artifact = Some(id);
        grant.allow_truncated = partial_confirmed;
        grant.cancelled = Some(cancelled);
        Ok(())
    }
    /// Host calls only after the user confirms that the exported JSONL is partial.
    /// The export prepends an explicit truncation record, so the marker survives transfer.
    pub fn authorize_truncated_export(
        &self,
        ctx: &RequestContext,
        handle: LeaseId,
        path: PathBuf,
        confirmed: bool,
    ) -> Result<(), PortError> {
        if !confirmed {
            return Err(PortError::TokenInvalid);
        }
        self.authorize_export(ctx, handle.clone(), path)?;
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        s.exports
            .get_mut(&handle)
            .ok_or(PortError::TokenInvalid)?
            .allow_truncated = true;
        Ok(())
    }
}
#[async_trait]
impl ArtifactRepository for FileArtifactRepository {
    async fn create(
        &self,
        ctx: &RequestContext,
        spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        self.create_sync(ctx, spec)
    }
    async fn append_chunk(
        &self,
        w: &mut ArtifactWriter,
        c: ChunkPayload,
    ) -> Result<ChunkIndex, PortError> {
        self.append_sync(w, c)
    }
    async fn describe(
        &self,
        c: &RequestContext,
        id: ArtifactId,
    ) -> Result<ArtifactMetadata, PortError> {
        self.describe_sync(c, &id)
    }
    async fn finalize(&self, w: ArtifactWriter) -> Result<ArtifactId, PortError> {
        self.finish_sync(w, None)
    }
    async fn abort(&self, w: ArtifactWriter, r: AbortReason) -> Result<ArtifactId, PortError> {
        self.finish_sync(w, Some(r))
    }
    async fn read_chunk(
        &self,
        c: &RequestContext,
        id: ArtifactId,
        i: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError> {
        self.read_sync(c, &id, i)
    }
    async fn read_range(
        &self,
        c: &RequestContext,
        id: ArtifactId,
        range: ByteRange,
    ) -> Result<ArtifactChunk, PortError> {
        let s = self.state.lock().map_err(|_| unavailable())?;
        let r = self.visible(&s, c, &id)?;
        let end = range.end.unwrap_or(r.metadata.published_byte_size.get());
        if end < range.start
            || end > r.metadata.published_byte_size.get()
            || end - range.start > 32 * 1024 * 1024
        {
            return Err(invalid());
        }
        let mut bytes = Vec::with_capacity((end - range.start) as usize);
        let mut offset = 0;
        for (index, size) in r.sizes.iter().enumerate() {
            let next = offset + size;
            if next > range.start && offset < end {
                let block = read_block(&self.dir(&id).join(format!("{index}.chunk")), *size)?;
                if block.len() as u64 != *size {
                    return Err(unavailable());
                }
                bytes.extend_from_slice(
                    &block[(range.start.saturating_sub(offset)) as usize
                        ..(end.min(next) - offset) as usize],
                );
            }
            offset = next;
        }
        Ok(ArtifactChunk {
            artifact_id: id,
            chunk_index: Counter::ZERO,
            total_chunks: r.metadata.total_chunks,
            offset: Counter::new(range.start),
            bytes,
            result_completeness: r.metadata.result_completeness,
        })
    }
    async fn export(
        &self,
        c: &RequestContext,
        id: ArtifactId,
        sink: ExportSink,
    ) -> Result<ExportReceipt, PortError> {
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        let r = self.visible(&s, c, &id)?.clone();
        let ExportSink::HostHandle(handle) = sink else {
            return Err(PortError::TokenInvalid);
        };
        let grant = s.exports.get(&handle).ok_or(PortError::TokenInvalid)?;
        if grant.expires <= now()
            || grant.organization != c.organization_id
            || grant.owner != c.principal_id
            || grant.artifact.as_ref().is_some_and(|bound| bound != &id)
        {
            return Err(PortError::TokenInvalid);
        }
        let truncated = r.metadata.lifecycle == ArtifactLifecycle::Truncated;
        if !r.metadata.lifecycle.allows_plain_export() && !(truncated && grant.allow_truncated) {
            return Err(invalid());
        }
        let grant = s.exports.remove(&handle).ok_or(PortError::TokenInvalid)?;
        drop(s);
        let path = grant.path;
        let check_cancelled = || {
            if grant
                .cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                Err(PortError::TokenInvalid)
            } else {
                Ok(())
            }
        };
        check_cancelled()?;
        let mut output = private_file(OpenOptions::new().write(true).create_new(true))
            .open(&path)
            .map_err(|_| unavailable())?;
        let mut exported_bytes = 0;
        let result = (|| {
            if truncated {
                let mut marker = serde_json::to_vec(&serde_json::json!({"artifactCompleteness":"truncated","truncationReason":r.metadata.truncation_reason})).map_err(|_| unavailable())?;
                marker.push(b'\n');
                output.write_all(&marker).map_err(|_| unavailable())?;
                exported_bytes += marker.len() as u64;
            }
            for index in 0..r.sizes.len() {
                check_cancelled()?;
                if grant.expires <= now() {
                    return Err(PortError::TokenInvalid);
                }
                let bytes = read_block(
                    &self.dir(&id).join(format!("{index}.chunk")),
                    r.sizes[index],
                )?;
                output.write_all(&bytes).map_err(|_| unavailable())?;
                exported_bytes += bytes.len() as u64;
            }
            check_cancelled()?;
            if grant.expires <= now() {
                return Err(PortError::TokenInvalid);
            }
            // Recheck revocation and artifact TTL at the publication boundary.
            let state = self.state.lock().map_err(|_| unavailable())?;
            self.visible(&state, c, &id)?;
            output.sync_all().map_err(|_| unavailable())
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result?;
        Ok(ExportReceipt {
            artifact_id: id,
            byte_length: exported_bytes,
            exported_at: Timestamp::new(
                chrono::DateTime::from_timestamp(now() as i64, 0)
                    .ok_or_else(unavailable)?
                    .to_rfc3339(),
            ),
        })
    }
    async fn delete_expired(&self, at: Timestamp) -> Result<Vec<ArtifactId>, PortError> {
        let time = chrono::DateTime::parse_from_rfc3339(at.as_str()).map_err(|_| invalid())?;
        self.expire_at(time.timestamp().max(0) as u64)
    }
    async fn revoke(
        &self,
        c: &RequestContext,
        id: ArtifactId,
        _reason: RevokeReason,
    ) -> Result<(), PortError> {
        let mut s = self.state.lock().map_err(|_| unavailable())?;
        let mut r = self.visible(&s, c, &id)?.clone();
        r.metadata.lifecycle = ArtifactLifecycle::Revoked;
        r.writer = None;
        self.commit(&r)?;
        s.records.insert(id, r);
        Ok(())
    }
}
