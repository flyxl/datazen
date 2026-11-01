//! `ArtifactRepository` 的服务端适配器（WR-D2 第四切片）。
//!
//! 字节半边是构造时给定的**受控目录**（`<root>/<artifact_id>/<index>.chunk`，
//! 打开期 0o700），元数据半边是管理库的 `artifact_metadata` 表——即
//! [team-server §10.1](../../../../../docs/architecture/platform/team-server-and-auth.md)
//! 的「字节存储 + 元数据表」组合。语义与内核 `FileArtifactRepository`
//! **逐方法对齐**（同一端口、另一适配器）：
//!
//! * create 绑定 ACL：spec 的组织/owner 必须与 `ctx` 一致、委托上下文拒绝、
//!   TTL 必须为正——否则 `TokenInvalid`；
//! * `append_chunk` 顺序分配 index，**字节与块元数据提交后才返回**；已发布
//!   块不可覆盖（文件 `create_new` + 行内 `chunk_sizes_json` 只追加）；
//! * 写入期已发布块可读；未发布索引/越界是参数错误（`CasConflict`），不是空块；
//! * 正常完成 `finalize` / 显式中止 `abort`（保留已发布块并标记 truncated）；
//! * `export` 只走 `ServerAuthorizedPath`（host 生成的受控路径）；桌面形态的
//!   `HostHandle` 授权句柄在本形态没有注册通道，一律 `TokenInvalid`；
//! * TTL 过期/吊销不可读；删除走墓碑表，宽限期内读取仍答 `ArtifactExpired`；
//! * 打开期恢复扫描：writing 行标记 truncated（WriterLost）、孤儿块文件回收、
//!   无元数据行的字节目录整体移除——「写字节/发布元数据之间的故障窗口」
//!   由「先写块文件、后事务提交元数据」关闭（§3.8 验收口径）。
//!
//! 与桌面实现的已记录差异：桌面 rename 落盘有「rename 已生效但目录 fsync
//! 失败」的窗口，需要保留已提交前缀；SQLite 事务提交是原子的（提交失败 =
//! 未生效），失败路径直接删块文件并返回错误，恢复扫描兜底。

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use datazen::execution::domain::artifact_repository::{
    ArtifactRepository, ArtifactSpec, ArtifactWriter, ByteRange, ChunkIndex, ChunkPayload,
    ExportSink, ARTIFACT_BYTE_QUOTA,
};
use datazen::execution::domain::{
    AbortReason, ArtifactChunk, ArtifactLifecycle, ArtifactMetadata, ResultCompleteness,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{ArtifactId, Counter, LeaseId, Timestamp};
use rusqlite::{params, OptionalExtension, Row};

use super::AdminStore;

/// 单块字节上限（与内核实现同值——契约口径，不是实现细节）。
const MAX_CHUNK_BYTES: u64 = 32 * 1024 * 1024;
/// 单产物块数上限。
const MAX_CHUNKS: usize = 65_536;
/// 墓碑宽限期：删除后一小时内读取仍答 `ArtifactExpired`。
const EXPIRED_GRACE_SECS: u64 = 3_600;

const ARTIFACT_COLUMNS: &str = "organization_id, artifact_id, owner_principal_id, execution_id, \
     delegated_execution, target_fingerprint, expires_epoch, lifecycle, published_chunk_count, \
     published_byte_size, total_chunks, result_completeness, truncation_reason, \
     chunk_sizes_json, writer_lease";

#[derive(Clone)]
pub struct ServerArtifacts {
    admin: Arc<AdminStore>,
    root: PathBuf,
}

/// 行的完整读取面（管理库 → 内核 `Record` 的等价形）。组织/owner/execution
/// 由 SQL 谓词与墓碑查询各自取用，不重复存进结构体。
struct ArtifactRow {
    delegated_execution: bool,
    fingerprint: String,
    expires_epoch: u64,
    metadata: ArtifactMetadata,
    sizes: Vec<u64>,
    writer_lease: Option<String>,
}

fn backend(source: impl std::fmt::Display) -> PortError {
    PortError::BackendUnavailable(source.to_string())
}

fn invalid_writer() -> PortError {
    PortError::cas_conflict("artifact", "invalid writer or unpublished range")
}

fn now_epoch() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

fn lifecycle_name(lifecycle: ArtifactLifecycle) -> &'static str {
    match lifecycle {
        ArtifactLifecycle::Writing => "writing",
        ArtifactLifecycle::Complete => "complete",
        ArtifactLifecycle::Truncated => "truncated",
        ArtifactLifecycle::Revoked => "revoked",
        ArtifactLifecycle::Expired => "expired",
    }
}

fn completeness_name(completeness: ResultCompleteness) -> &'static str {
    match completeness {
        ResultCompleteness::Pending => "pending",
        ResultCompleteness::Complete => "complete",
        ResultCompleteness::Truncated => "truncated",
    }
}

fn truncation_name(reason: AbortReason) -> &'static str {
    match reason {
        AbortReason::WriterLost => "writerLost",
        AbortReason::ExecutionFailed => "executionFailed",
        AbortReason::Cancelled => "cancelled",
        AbortReason::ConsumerLimitExceeded => "consumerLimitExceeded",
    }
}

#[cfg(unix)]
fn private_dir(path: &Path) -> Result<(), PortError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| {
        backend("artifact dir permissions")
    })
}

#[cfg(not(unix))]
fn private_dir(_path: &Path) -> Result<(), PortError> {
    Ok(())
}

#[cfg(unix)]
fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    options
}

#[cfg(not(unix))]
fn private_file(options: &mut OpenOptions) -> &mut OpenOptions {
    options
}

fn row_from_row(row: &Row<'_>) -> rusqlite::Result<ArtifactRow> {
    let invalid = |name: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                name.to_string(),
            )),
        )
    };
    // 列 0/2/3（organization/owner/execution）由 SQL 谓词消费，此处按位跳过。
    let _organization: String = row.get(0)?;
    let artifact_id: String = row.get(1)?;
    let _owner: String = row.get(2)?;
    let _execution: String = row.get(3)?;
    let delegated: i64 = row.get(4)?;
    let fingerprint: String = row.get(5)?;
    let expires_epoch: i64 = row.get(6)?;
    let lifecycle_name: String = row.get(7)?;
    let published_chunk_count: i64 = row.get(8)?;
    let published_byte_size: i64 = row.get(9)?;
    let total_chunks: Option<i64> = row.get(10)?;
    let completeness_name: String = row.get(11)?;
    let truncation_reason: Option<String> = row.get(12)?;
    let sizes_json: String = row.get(13)?;
    let writer_lease: Option<String> = row.get(14)?;

    let lifecycle = match lifecycle_name.as_str() {
        "writing" => ArtifactLifecycle::Writing,
        "complete" => ArtifactLifecycle::Complete,
        "truncated" => ArtifactLifecycle::Truncated,
        "revoked" => ArtifactLifecycle::Revoked,
        "expired" => ArtifactLifecycle::Expired,
        _ => return Err(invalid("artifact lifecycle")),
    };
    let result_completeness = match completeness_name.as_str() {
        "pending" => ResultCompleteness::Pending,
        "complete" => ResultCompleteness::Complete,
        "truncated" => ResultCompleteness::Truncated,
        _ => return Err(invalid("artifact completeness")),
    };
    let truncation_reason = match truncation_reason.as_deref() {
        None => None,
        Some("writerLost") => Some(AbortReason::WriterLost),
        Some("executionFailed") => Some(AbortReason::ExecutionFailed),
        Some("cancelled") => Some(AbortReason::Cancelled),
        Some("consumerLimitExceeded") => Some(AbortReason::ConsumerLimitExceeded),
        _ => return Err(invalid("truncation reason")),
    };
    let sizes: Vec<u64> =
        serde_json::from_str(&sizes_json).map_err(|_| invalid("chunk sizes"))?;

    Ok(ArtifactRow {
        delegated_execution: delegated != 0,
        fingerprint: fingerprint.clone(),
        expires_epoch: expires_epoch.max(0) as u64,
        metadata: ArtifactMetadata {
            target_fingerprint: fingerprint.clone(),
            artifact_id: ArtifactId::new(&artifact_id),
            lifecycle,
            published_chunk_count: Counter::new(published_chunk_count.max(0) as u64),
            published_byte_size: Counter::new(published_byte_size.max(0) as u64),
            total_chunks: total_chunks.map(|value| Counter::new(value.max(0) as u64)),
            result_completeness,
            truncation_reason,
        },
        sizes,
        writer_lease,
    })
}

impl ServerArtifacts {
    /// 打开受控目录并执行恢复扫描。调用方负责先跑 `AdminStore::migrate`。
    pub fn open(admin: Arc<AdminStore>, root: impl AsRef<Path>) -> Result<Self, PortError> {
        if fs::symlink_metadata(root.as_ref()).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(backend("artifact root must not be a symlink"));
        }
        fs::create_dir_all(root.as_ref()).map_err(|_| backend("artifact root create"))?;
        private_dir(root.as_ref())?;
        let this = Self {
            admin,
            root: root.as_ref().to_path_buf(),
        };
        this.recover()?;
        Ok(this)
    }

    fn dir(&self, id: &ArtifactId) -> PathBuf {
        self.root.join(id.as_str())
    }

    fn chunk_path(&self, id: &ArtifactId, index: u64) -> PathBuf {
        self.dir(id).join(format!("{index}.chunk"))
    }

    /// 恢复扫描（打开期一次性）：
    /// 1. 行在、目录/块文件不齐 → 整个产物移除（与内核 `open` 的 valid 检查同口径）；
    /// 2. writing 行 → truncated（WriterLost），total_chunks 固化为已发布数；
    /// 3. index ≥ 已发布数的孤儿块文件移除（崩溃窗口的未发布字节）；
    /// 4. 无元数据行的目录 → 整体移除。
    fn recover(&self) -> Result<(), PortError> {
        let rows: Vec<ArtifactRow> = {
            let conn = self.lock()?;
            let mut stmt = conn
                .prepare(&format!("SELECT {ARTIFACT_COLUMNS} FROM artifact_metadata"))
                .map_err(backend)?;
            let mapped = stmt
                .query_map([], row_from_row)
                .map_err(backend)?;
            mapped
                .collect::<Result<Vec<_>, _>>()
                .map_err(backend)?
        };
        for row in rows {
            let id = row.metadata.artifact_id.clone();
            let dir = self.dir(&id);
            let files_valid = row.sizes.len() <= MAX_CHUNKS
                && row.sizes.iter().enumerate().all(|(index, size)| {
                    fs::symlink_metadata(self.chunk_path(&id, index as u64))
                        .is_ok_and(|meta| meta.is_file() && meta.len() == *size)
                });
            if !files_valid {
                let _ = fs::remove_dir_all(&dir);
                let conn = self.lock()?;
                conn.execute(
                    "DELETE FROM artifact_metadata WHERE artifact_id = ?1",
                    params![id.as_str()],
                )
                .map_err(backend)?;
                continue;
            }
            let mut updated = row.metadata.clone();
            if updated.lifecycle == ArtifactLifecycle::Writing {
                updated.lifecycle = ArtifactLifecycle::Truncated;
                updated.result_completeness = ResultCompleteness::Truncated;
                updated.truncation_reason = Some(AbortReason::WriterLost);
                updated.total_chunks = Some(updated.published_chunk_count);
                let conn = self.lock()?;
                Self::sync_write_lifecycle(&conn, id.as_str(), &updated, None)?;
            }
            // 崩溃窗口的未发布字节：绝不变得可见，也不阻塞未来的存储预留。
            // 按目录清单逐个核对（与内核 open 的孤儿块扫描同口径，不假设
            // 孤儿只出现在已发布数的紧后一位）。
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(index) = name
                        .strip_suffix(".chunk")
                        .and_then(|value| value.parse::<u64>().ok())
                    {
                        if index >= row.sizes.len() as u64 {
                            fs::remove_file(entry.path())
                                .map_err(|_| backend("orphan chunk remove"))?;
                        }
                    }
                }
            }
        }
        for item in fs::read_dir(&self.root).map_err(|_| backend("artifact root read"))? {
            let item = item.map_err(|_| backend("artifact root read"))?;
            if !item.file_type().map_err(|_| backend("artifact root read"))?.is_dir() {
                continue;
            }
            let name = item.file_name().to_string_lossy().to_string();
            let conn = self.lock()?;
            let known: bool = conn
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM artifact_metadata WHERE artifact_id = ?1)",
                    params![name],
                    |row| row.get(0),
                )
                .map_err(backend)?;
            if !known {
                fs::remove_dir_all(item.path()).map_err(|_| backend("orphan dir remove"))?;
            }
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, PortError> {
        self.admin
            .lock_for_adapter()
            .map_err(|source| backend(source))
    }

    /// 生命周期写回（恢复扫描与 revoke 共用；不含 sizes/计数推进）。
    fn sync_write_lifecycle(
        conn: &rusqlite::Connection,
        artifact_id: &str,
        metadata: &ArtifactMetadata,
        writer_lease: Option<String>,
    ) -> Result<(), PortError> {
        conn.execute(
            "UPDATE artifact_metadata SET lifecycle = ?2, result_completeness = ?3,
                    total_chunks = ?4, truncation_reason = ?5, writer_lease = ?6
             WHERE artifact_id = ?1",
            params![
                artifact_id,
                lifecycle_name(metadata.lifecycle),
                completeness_name(metadata.result_completeness),
                metadata.total_chunks.map(|value| value.get() as i64),
                metadata.truncation_reason.map(truncation_name),
                writer_lease,
            ],
        )
        .map_err(backend)?;
        Ok(())
    }

    /// 可见性：墓碑 → `ArtifactExpired`；行不在/越组织 → `NotFound`；TTL 到期 →
    /// `ArtifactExpired`；吊销 → `NotFound`（与内核 `visible` 同口径）。
    fn sync_visible(
        conn: &rusqlite::Connection,
        ctx: &RequestContext,
        id: &ArtifactId,
    ) -> Result<ArtifactRow, PortError> {
        let tombstone: Option<(String, String)> = conn
            .query_row(
                "SELECT organization_id, owner_principal_id FROM expired_artifact
                 WHERE artifact_id = ?1",
                params![id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(backend)?;
        if let Some((organization, owner)) = tombstone {
            if organization == ctx.organization_id.as_str()
                && owner == ctx.principal_id.as_str()
                && ctx.delegation_id.is_none()
            {
                return Err(PortError::ArtifactExpired);
            }
        }
        let row = conn
            .query_row(
                &format!(
                    "SELECT {ARTIFACT_COLUMNS} FROM artifact_metadata
                     WHERE organization_id = ?1 AND artifact_id = ?2"
                ),
                params![ctx.organization_id.as_str(), id.as_str()],
                row_from_row,
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound("artifact".into()))?;
        if row.delegated_execution || ctx.delegation_id.is_some() {
            // 委托结果是进程内已准入的能力，端口读取路径不接受委托上下文
            // （与内核 `visible` 的过滤同口径）。
            return Err(PortError::NotFound("artifact".into()));
        }
        if row.expires_epoch <= now_epoch()
            || row.metadata.lifecycle == ArtifactLifecycle::Expired
        {
            return Err(PortError::ArtifactExpired);
        }
        if row.metadata.lifecycle == ArtifactLifecycle::Revoked {
            return Err(PortError::NotFound("artifact".into()));
        }
        Ok(row)
    }

    /// writer 校验：行存在、未过期、lease 匹配、lifecycle=writing、计数一致。
    /// writer 路径不做组织比对（与内核 `writer` 同口径：句柄本身即能力）。
    fn sync_writer_row(
        conn: &rusqlite::Connection,
        writer: &ArtifactWriter,
    ) -> Result<ArtifactRow, PortError> {
        let row = conn
            .query_row(
                &format!(
                    "SELECT {ARTIFACT_COLUMNS} FROM artifact_metadata WHERE artifact_id = ?1"
                ),
                params![writer.artifact_id.as_str()],
                row_from_row,
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(invalid_writer)?;
        if row.expires_epoch <= now_epoch() {
            return Err(PortError::ArtifactExpired);
        }
        if row.writer_lease.as_deref() != Some(writer.lease_id.as_str())
            || row.metadata.lifecycle != ArtifactLifecycle::Writing
            || writer.next_chunk_index != row.metadata.published_chunk_count
            || writer.published_chunk_count != row.metadata.published_chunk_count
            || writer.published_byte_size != row.metadata.published_byte_size
        {
            return Err(invalid_writer());
        }
        Ok(row)
    }

    fn sync_create(
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
        let artifact_id = ArtifactId::new(uuid::Uuid::new_v4().to_string());
        let lease = LeaseId::new(uuid::Uuid::new_v4().to_string());
        let expires_epoch = now_epoch().saturating_add(spec.ttl_seconds);

        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        tx.execute(
            "INSERT INTO artifact_metadata (
               organization_id, artifact_id, owner_principal_id, execution_id,
               delegated_execution, target_fingerprint, expires_epoch, lifecycle,
               published_chunk_count, published_byte_size, total_chunks,
               result_completeness, truncation_reason, chunk_sizes_json,
               writer_lease, created_at
             ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, 'writing',
               0, 0, NULL, 'pending', NULL, '[]', ?7, ?8)",
            params![
                ctx.organization_id.as_str(),
                artifact_id.as_str(),
                spec.owner_principal_id.as_str(),
                spec.execution_id.as_str(),
                spec.target_fingerprint,
                expires_epoch as i64,
                lease.as_str(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        fs::create_dir_all(self.dir(&artifact_id)).map_err(|_| backend("artifact dir create"))?;
        private_dir(&self.dir(&artifact_id))?;

        Ok((
            artifact_id.clone(),
            ArtifactWriter {
                artifact_id,
                lease_id: lease,
                next_chunk_index: Counter::ZERO,
                published_chunk_count: Counter::ZERO,
                published_byte_size: Counter::ZERO,
            },
        ))
    }

    fn sync_append(
        &self,
        writer: &mut ArtifactWriter,
        chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError> {
        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let row = Self::sync_writer_row(&tx, writer)?;

        let total: i64 = tx
            .query_row(
                "SELECT COALESCE(SUM(published_byte_size), 0) FROM artifact_metadata",
                [],
                |row| row.get(0),
            )
            .map_err(backend)?;
        let size = chunk.bytes.len() as u64;
        if size > MAX_CHUNK_BYTES
            || (total.max(0) as u64).saturating_add(size) > ARTIFACT_BYTE_QUOTA
            || row.sizes.len() >= MAX_CHUNKS
        {
            return Err(PortError::QuotaExceeded("artifact byte budget".into()));
        }

        let index = writer.next_chunk_index;
        // 先写块文件（create_new：已发布 index 复用在此处硬失败），提交成功才发布。
        let path = self.chunk_path(&writer.artifact_id, index.get());
        let mut file = private_file(OpenOptions::new().write(true).create_new(true))
            .open(&path)
            .map_err(|_| backend("chunk file create"))?;
        if file.write_all(&chunk.bytes).and_then(|_| file.sync_all()).is_err() {
            let _ = fs::remove_file(&path);
            return Err(backend("chunk file write"));
        }

        let sizes_json = {
            let mut sizes = row.sizes.clone();
            sizes.push(size);
            serde_json::to_string(&sizes).map_err(|_| backend("sizes serialization"))?
        };
        let next_count = writer.published_chunk_count.saturating_increment();
        let next_bytes = writer.published_byte_size.saturating_increment_by(size);
        let updated = tx
            .execute(
                "UPDATE artifact_metadata SET chunk_sizes_json = ?3,
                        published_chunk_count = ?4, published_byte_size = ?5
                 WHERE artifact_id = ?1 AND writer_lease = ?2 AND lifecycle = 'writing'",
                params![
                    writer.artifact_id.as_str(),
                    writer.lease_id.as_str(),
                    sizes_json,
                    next_count.get() as i64,
                    next_bytes.get() as i64,
                ],
            )
            .map_err(backend)?;
        if updated == 0 {
            let _ = fs::remove_file(&path);
            return Err(invalid_writer());
        }
        tx.commit().map_err(backend)?;

        writer.next_chunk_index = next_count;
        writer.published_chunk_count = next_count;
        writer.published_byte_size = next_bytes;
        Ok(index)
    }

    fn sync_describe(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
    ) -> Result<ArtifactMetadata, PortError> {
        let conn = self.lock()?;
        let row = Self::sync_visible(&conn, ctx, id)?;
        let mut metadata = row.metadata.clone();
        metadata.target_fingerprint = row.fingerprint.clone();
        Ok(metadata)
    }

    fn sync_finish(
        &self,
        writer: ArtifactWriter,
        reason: Option<AbortReason>,
    ) -> Result<ArtifactId, PortError> {
        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let row = Self::sync_writer_row(&tx, &writer)?;
        let mut metadata = row.metadata.clone();
        metadata.lifecycle = if reason.is_some() {
            ArtifactLifecycle::Truncated
        } else {
            ArtifactLifecycle::Complete
        };
        metadata.result_completeness = if reason.is_some() {
            ResultCompleteness::Truncated
        } else {
            ResultCompleteness::Complete
        };
        metadata.truncation_reason = reason;
        metadata.total_chunks = Some(metadata.published_chunk_count);
        // SQLite 提交原子：失败即未生效，无需桌面 rename 窗口的「保留已提交
        // 前缀」 dance；行保持 writing，调用方可重试或由恢复扫描收口。
        Self::sync_write_lifecycle(&tx, writer.artifact_id.as_str(), &metadata, None)?;
        tx.commit().map_err(backend)?;
        Ok(writer.artifact_id)
    }

    fn sync_read_chunk(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
        index: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError> {
        let conn = self.lock()?;
        let row = Self::sync_visible(&conn, ctx, id)?;
        if !row.metadata.is_published(index) {
            return Err(invalid_writer());
        }
        let slot = index.get() as usize;
        let size = *row.sizes.get(slot).ok_or_else(invalid_writer)?;
        let bytes = Self::read_block(&self.chunk_path(id, index.get()), size)?;
        let offset: u64 = row.sizes.iter().take(slot).sum();
        Ok(ArtifactChunk {
            artifact_id: id.clone(),
            chunk_index: index,
            total_chunks: row.metadata.total_chunks,
            offset: Counter::new(offset),
            bytes,
            result_completeness: row.metadata.result_completeness,
        })
    }

    fn sync_read_range(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
        range: ByteRange,
    ) -> Result<ArtifactChunk, PortError> {
        let conn = self.lock()?;
        let row = Self::sync_visible(&conn, ctx, id)?;
        let end = range.end.unwrap_or(row.metadata.published_byte_size.get());
        if end < range.start
            || end > row.metadata.published_byte_size.get()
            || end - range.start > MAX_CHUNK_BYTES
        {
            return Err(invalid_writer());
        }
        let mut bytes = Vec::with_capacity((end - range.start) as usize);
        let mut offset = 0u64;
        for (index, size) in row.sizes.iter().enumerate() {
            let next = offset + size;
            if next > range.start && offset < end {
                let block = Self::read_block(&self.chunk_path(id, index as u64), *size)?;
                bytes.extend_from_slice(
                    &block[(range.start.saturating_sub(offset)) as usize
                        ..(end.min(next) - offset) as usize],
                );
            }
            offset = next;
        }
        Ok(ArtifactChunk {
            artifact_id: id.clone(),
            chunk_index: Counter::ZERO,
            total_chunks: row.metadata.total_chunks,
            offset: Counter::new(range.start),
            bytes,
            result_completeness: row.metadata.result_completeness,
        })
    }

    fn sync_export(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
        sink: ExportSink,
    ) -> Result<datazen::execution::domain::artifact_repository::ExportReceipt, PortError> {
        use datazen::execution::domain::artifact_repository::ExportReceipt;

        // 可见性检查在独立作用域完成：导出的字节循环与发布边界复检各自
        // 取锁（单 Mutex 不可重入，持锁跨循环会自锁）。
        let row = {
            let conn = self.lock()?;
            Self::sync_visible(&conn, ctx, id)?
        };
        // 服务端受控出口：路径由 host 生成（不来自请求体），组织必须一致；
        // 委托上下文不可导出。桌面形态的 HostHandle 授权句柄在本形态没有
        // 注册通道，一律 TokenInvalid。truncated 的明确确认通道
        // （桌面走 grant.allow_truncated）随受控上传/下载切片（WR-D7）落地。
        let path = match sink {
            ExportSink::ServerAuthorizedPath(organization, path)
                if organization == ctx.organization_id && ctx.delegation_id.is_none() =>
            {
                PathBuf::from(path)
            }
            _ => return Err(PortError::TokenInvalid),
        };
        if !row.metadata.lifecycle.allows_plain_export() {
            return Err(invalid_writer());
        }
        let mut output = private_file(OpenOptions::new().write(true).create_new(true))
            .open(&path)
            .map_err(|_| backend("export target create"))?;
        let mut exported_bytes = 0u64;
        let result = (|| {
            for (index, size) in row.sizes.iter().enumerate() {
                let bytes = Self::read_block(&self.chunk_path(id, index as u64), *size)?;
                output.write_all(&bytes).map_err(|_| backend("export write"))?;
                exported_bytes += bytes.len() as u64;
            }
            // 发布边界复检：TTL/吊销在导出期间变化则丢弃产物。
            let conn = self.lock()?;
            Self::sync_visible(&conn, ctx, id)?;
            output.sync_all().map_err(|_| backend("export sync"))
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(&path);
        }
        result?;
        Ok(ExportReceipt {
            artifact_id: id.clone(),
            byte_length: exported_bytes,
            exported_at: Timestamp::new(&chrono::Utc::now().to_rfc3339()),
        })
    }

    fn sync_delete_expired(&self, at: Timestamp) -> Result<Vec<ArtifactId>, PortError> {
        let parsed = chrono::DateTime::parse_from_rfc3339(at.as_str())
            .map_err(|_| invalid_writer())?;
        let at_epoch = parsed.timestamp().max(0) as u64;
        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        // 单连接锁下无并发写：候选集直接在锁内读出（不进事务借用期）。
        let expired: Vec<(String, String, String)> = {
            let mut stmt = tx
                .prepare(
                    "SELECT artifact_id, organization_id, owner_principal_id
                     FROM artifact_metadata WHERE expires_epoch <= ?1",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![at_epoch as i64], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .map_err(backend)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(backend)?
        };
        let mut ids = Vec::with_capacity(expired.len());
        for (artifact_id, organization, owner) in &expired {
            let _ = fs::remove_dir_all(self.root.join(artifact_id));
            tx.execute(
                "DELETE FROM artifact_metadata WHERE artifact_id = ?1",
                params![artifact_id],
            )
            .map_err(backend)?;
            tx.execute(
                "INSERT INTO expired_artifact (artifact_id, organization_id, owner_principal_id, expires_epoch)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (artifact_id) DO UPDATE SET expires_epoch = excluded.expires_epoch",
                params![artifact_id, organization, owner, (at_epoch + EXPIRED_GRACE_SECS) as i64],
            )
            .map_err(backend)?;
            ids.push(ArtifactId::new(artifact_id));
        }
        tx.execute(
            "DELETE FROM expired_artifact WHERE expires_epoch <= ?1",
            params![at_epoch as i64],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(ids)
    }

    fn sync_revoke(
        &self,
        ctx: &RequestContext,
        id: &ArtifactId,
        _reason: datazen::execution::domain::artifact_repository::RevokeReason,
    ) -> Result<(), PortError> {
        let conn = self.lock()?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let row = Self::sync_visible(&tx, ctx, id)?;
        let mut metadata = row.metadata.clone();
        metadata.lifecycle = ArtifactLifecycle::Revoked;
        Self::sync_write_lifecycle(&tx, id.as_str(), &metadata, None)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn read_block(path: &Path, size: u64) -> Result<Vec<u8>, PortError> {
        if size > MAX_CHUNK_BYTES {
            return Err(backend("chunk size cap"));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        let file = private_file(&mut options)
            .open(path)
            .map_err(|_| backend("chunk file open"))?;
        let meta = file.metadata().map_err(|_| backend("chunk file metadata"))?;
        if !meta.is_file() || meta.len() != size {
            return Err(backend("chunk file size mismatch"));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        file.take(size + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| backend("chunk file read"))?;
        if bytes.len() as u64 != size {
            return Err(backend("chunk file truncated"));
        }
        Ok(bytes)
    }
}

/// 异步边界：内核 trait 是 async，管理库 + 受控目录是同步——每个 trait 方法
/// 把同步核心丢进 `spawn_blocking`，差异只允许出现在这一层。
#[async_trait]
impl ArtifactRepository for ServerArtifacts {
    async fn create(
        &self,
        ctx: &RequestContext,
        spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_create(&ctx, spec))
            .await
            .map_err(backend)?
    }

    async fn append_chunk(
        &self,
        writer: &mut ArtifactWriter,
        chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError> {
        let this = self.clone();
        let mut handle = writer.clone();
        let (handle, outcome) =
            tokio::task::spawn_blocking(move || {
                let outcome = this.sync_append(&mut handle, chunk);
                (handle, outcome)
            })
            .await
            .map_err(backend)?;
        // 计数推进写回调用方句柄（同步核心已保证失败路径不动计数）。
        writer.next_chunk_index = handle.next_chunk_index;
        writer.published_chunk_count = handle.published_chunk_count;
        writer.published_byte_size = handle.published_byte_size;
        outcome
    }

    async fn describe(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadata, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_describe(&ctx, &artifact_id))
            .await
            .map_err(backend)?
    }

    async fn finalize(&self, writer: ArtifactWriter) -> Result<ArtifactId, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_finish(writer, None))
            .await
            .map_err(backend)?
    }

    async fn abort(
        &self,
        writer: ArtifactWriter,
        reason: datazen::execution::domain::AbortReason,
    ) -> Result<ArtifactId, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_finish(writer, Some(reason)))
            .await
            .map_err(backend)?
    }

    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        chunk_index: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_read_chunk(&ctx, &artifact_id, chunk_index)
        })
        .await
        .map_err(backend)?
    }

    async fn read_range(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        range: ByteRange,
    ) -> Result<ArtifactChunk, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_read_range(&ctx, &artifact_id, range))
            .await
            .map_err(backend)?
    }

    async fn export(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        sink: ExportSink,
    ) -> Result<datazen::execution::domain::artifact_repository::ExportReceipt, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_export(&ctx, &artifact_id, sink))
            .await
            .map_err(backend)?
    }

    async fn delete_expired(&self, now: Timestamp) -> Result<Vec<ArtifactId>, PortError> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.sync_delete_expired(now))
            .await
            .map_err(backend)?
    }

    async fn revoke(
        &self,
        ctx: &RequestContext,
        artifact_id: ArtifactId,
        reason: datazen::execution::domain::artifact_repository::RevokeReason,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_revoke(&ctx, &artifact_id, reason))
            .await
            .map_err(backend)?
    }
}
