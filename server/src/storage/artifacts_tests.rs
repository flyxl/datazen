//! `ServerArtifacts` 行为钉子：与内核 `FileArtifactRepository` 同一端口契约的
//! 服务端腿（「字节存储 + 元数据表」组合，受控目录 + 管理库）。
//!
//! 钉的语义：create 的 ACL 绑定、发布即追加（已发布块不可覆盖）、写入期
//! 已发布块可读、未发布索引是参数错误、finalize/abort 的生命周期分岔、
//! 受控导出只走 ServerAuthorizedPath、TTL 墓碑、恢复扫描的三个口径
//! （writing → truncated、孤儿块回收、无行目录移除）。

use std::fs;
use std::sync::Arc;

use datazen::execution::domain::artifact_repository::{
    ArtifactRepository, ArtifactSpec, ByteRange, ChunkPayload, ExportSink,
};
use datazen::execution::domain::{ArtifactLifecycle, ResultCompleteness};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::{
    ClientInstanceId, ExecutionId, OrganizationId, PrincipalId, RequestId, Timestamp,
};

use super::artifacts::ServerArtifacts;
use super::AdminStore;

fn migrated_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn ctx(organization: &str, principal: &str) -> RequestContext {
    RequestContext {
        organization_id: OrganizationId::new(organization),
        principal_id: PrincipalId::new(principal),
        authentication_session_id: None,
        client_instance_id: ClientInstanceId::new("test-client"),
        request_id: RequestId::new("req-test"),
        delegation_id: None,
    }
}

fn unique_root(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("datazen-artifacts-{tag}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("temp root");
    dir
}

fn spec() -> ArtifactSpec {
    ArtifactSpec::new(
        OrganizationId::new("org-a"),
        PrincipalId::new("user-1"),
        ExecutionId::new("exec-1"),
        "conn-1/public/t/public.t".to_string(),
    )
}

#[tokio::test]
async fn create_binds_the_acl_from_the_request_context() {
    let store = migrated_store();
    let root = unique_root("acl");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");

    // spec 自报的组织/主体必须与 ctx 一致（ACL 在 create 时绑定）。
    let forged = ArtifactSpec::new(
        OrganizationId::new("org-other"),
        PrincipalId::new("user-1"),
        ExecutionId::new("exec-1"),
        "fp".into(),
    );
    let outcome = artifacts.create(&ctx("org-a", "user-1"), forged).await;
    assert!(matches!(outcome, Err(PortError::TokenInvalid)));

    // TTL 必须为正。
    let mut zero_ttl = spec();
    zero_ttl.ttl_seconds = 0;
    let outcome = artifacts.create(&ctx("org-a", "user-1"), zero_ttl).await;
    assert!(matches!(outcome, Err(PortError::TokenInvalid)));

    let (artifact_id, writer) = artifacts
        .create(&ctx("org-a", "user-1"), spec())
        .await
        .expect("create");
    assert_eq!(writer.next_chunk_index.get(), 0);
    let meta = artifacts
        .describe(&ctx("org-a", "user-1"), artifact_id)
        .await
        .expect("describe during writing");
    assert_eq!(meta.lifecycle, ArtifactLifecycle::Writing);
    assert_eq!(meta.result_completeness, ResultCompleteness::Pending);
    assert_eq!(
        meta.total_chunks, None,
        "未终结时 totalChunks 是 null 不是 0"
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn published_chunks_are_readable_before_finalization_and_immutable() {
    let store = migrated_store();
    let root = unique_root("publish");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");
    let context = ctx("org-a", "user-1");
    let (artifact_id, mut writer) = artifacts.create(&context, spec()).await.expect("create");

    let stale = writer.clone();
    let first = artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![1, 2, 3]))
        .await
        .expect("first chunk");
    assert_eq!(first.get(), 0);

    // 已发布块写入期可读（读取权随发布走）。
    let chunk = artifacts
        .read_chunk(&context, artifact_id.clone(), first)
        .await
        .expect("published chunk readable");
    assert_eq!(chunk.bytes, vec![1, 2, 3]);
    assert_eq!(chunk.offset.get(), 0);

    // 旧计数句柄的写入是参数错误（已发布块不可覆盖，writer 不可回退）。
    let outcome = artifacts
        .append_chunk(&mut stale.clone(), ChunkPayload::new(vec![9]))
        .await;
    assert!(matches!(
        outcome,
        Err(PortError::CasConflict {
            entity: "artifact",
            ..
        })
    ));

    // 未发布索引是参数错误，不是空块。
    let outcome = artifacts
        .read_chunk(
            &context,
            artifact_id.clone(),
            datazen::shared::id::Counter::new(5),
        )
        .await;
    assert!(matches!(outcome, Err(PortError::CasConflict { .. })));

    artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![4, 5]))
        .await
        .expect("second chunk");
    let range = artifacts
        .read_range(&context, artifact_id.clone(), ByteRange::from(0))
        .await
        .expect("range read");
    assert_eq!(range.bytes, vec![1, 2, 3, 4, 5]);

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn finalize_completes_and_export_flows_through_the_authorized_path_only() {
    let store = migrated_store();
    let root = unique_root("export");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");
    let context = ctx("org-a", "user-1");
    let (artifact_id, mut writer) = artifacts.create(&context, spec()).await.expect("create");
    artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![7, 8]))
        .await
        .expect("chunk 1");
    artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![9]))
        .await
        .expect("chunk 2");

    let finished = artifacts.finalize(writer.clone()).await.expect("finalize");
    assert_eq!(finished, artifact_id);

    // 终结后的 writer 句柄作废。
    let outcome = artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![1]))
        .await;
    assert!(matches!(outcome, Err(PortError::CasConflict { .. })));

    let target = unique_root("export-out").join("result.bin");
    let receipt = artifacts
        .export(
            &context,
            artifact_id.clone(),
            ExportSink::ServerAuthorizedPath(
                OrganizationId::new("org-a"),
                target.display().to_string(),
            ),
        )
        .await
        .expect("authorized export");
    assert_eq!(receipt.byte_length, 3);
    assert_eq!(fs::read(&target).expect("exported bytes"), vec![7, 8, 9]);

    // HostHandle 是桌面形态的授权句柄；服务端没有注册通道。
    let outcome = artifacts
        .export(
            &context,
            artifact_id.clone(),
            ExportSink::HostHandle(datazen::shared::id::LeaseId::new("dialog-1")),
        )
        .await;
    assert!(matches!(outcome, Err(PortError::TokenInvalid)));

    // 别组织授权的路径不可用（sink 组织与 ctx 不一致）。
    let outcome = artifacts
        .export(
            &context,
            artifact_id.clone(),
            ExportSink::ServerAuthorizedPath(
                OrganizationId::new("org-b"),
                target.display().to_string(),
            ),
        )
        .await;
    assert!(matches!(outcome, Err(PortError::TokenInvalid)));

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(target.parent().unwrap()).ok();
}

#[tokio::test]
async fn abort_keeps_published_chunks_and_marks_truncation_reason() {
    let store = migrated_store();
    let root = unique_root("abort");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");
    let context = ctx("org-a", "user-1");
    let (artifact_id, mut writer) = artifacts.create(&context, spec()).await.expect("create");
    artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![1]))
        .await
        .expect("chunk");

    artifacts
        .abort(writer, datazen::execution::domain::AbortReason::Cancelled)
        .await
        .expect("abort");

    let meta = artifacts
        .describe(&context, artifact_id.clone())
        .await
        .expect("describe after abort");
    assert_eq!(meta.lifecycle, ArtifactLifecycle::Truncated);
    assert_eq!(
        meta.truncation_reason,
        Some(datazen::execution::domain::AbortReason::Cancelled)
    );
    assert_eq!(
        meta.total_chunks,
        Some(datazen::shared::id::Counter::new(1))
    );

    // 已发布块保留：截断不是清空。
    let chunk = artifacts
        .read_chunk(
            &context,
            artifact_id.clone(),
            datazen::shared::id::Counter::ZERO,
        )
        .await
        .expect("published chunk survives abort");
    assert_eq!(chunk.bytes, vec![1]);

    // truncated 不走常规导出（确认通道随受控上传/下载切片落地）。
    let target = unique_root("abort-out").join("x.bin");
    let outcome = artifacts
        .export(
            &context,
            artifact_id,
            ExportSink::ServerAuthorizedPath(
                OrganizationId::new("org-a"),
                target.display().to_string(),
            ),
        )
        .await;
    assert!(matches!(outcome, Err(PortError::CasConflict { .. })));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn cross_organization_reads_and_revocation() {
    let store = migrated_store();
    let root = unique_root("org");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");
    let context = ctx("org-a", "user-1");
    let (artifact_id, writer) = artifacts.create(&context, spec()).await.expect("create");

    // 越组织读取按「不存在」处理（CM-05），不泄漏存在性。
    let outcome = artifacts
        .describe(&ctx("org-b", "user-2"), artifact_id.clone())
        .await;
    assert!(matches!(outcome, Err(PortError::NotFound(_))));

    artifacts
        .revoke(
            &context,
            artifact_id.clone(),
            datazen::execution::domain::artifact_repository::RevokeReason::PolicyDenied,
        )
        .await
        .expect("revoke");
    let outcome = artifacts.describe(&context, artifact_id).await;
    assert!(
        matches!(outcome, Err(PortError::NotFound(_))),
        "吊销后不可读"
    );

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn expired_artifacts_answer_artifact_expired_within_the_grace_window() {
    let store = migrated_store();
    let root = unique_root("ttl");
    let artifacts = ServerArtifacts::open(store, &root).expect("open");
    let context = ctx("org-a", "user-1");
    let (artifact_id, mut writer) = artifacts.create(&context, spec()).await.expect("create");
    artifacts
        .append_chunk(&mut writer, ChunkPayload::new(vec![3]))
        .await
        .expect("chunk");

    // 推进到产物 TTL 之后清理：产物删除、墓碑落库。
    let future = Timestamp::new("2027-01-01T00:00:00Z");
    let reclaimed = artifacts.delete_expired(future).await.expect("sweep");
    assert_eq!(reclaimed, vec![artifact_id.clone()]);

    // 宽限期内读取回答 ArtifactExpired（不是 NotFound）。
    let outcome = artifacts.describe(&context, artifact_id.clone()).await;
    assert!(matches!(outcome, Err(PortError::ArtifactExpired)));

    // 墓碑也过期后按「不存在」处理。
    let after_grace = Timestamp::new("2027-01-01T02:00:00Z");
    artifacts
        .delete_expired(after_grace)
        .await
        .expect("tombstone sweep");
    let outcome = artifacts.describe(&context, artifact_id).await;
    assert!(matches!(outcome, Err(PortError::NotFound(_))));

    fs::remove_dir_all(&root).ok();
}

#[tokio::test]
async fn reopening_marks_lost_writers_truncated_and_reclaims_orphans() {
    let store = migrated_store();
    let root = unique_root("recover");
    let context = ctx("org-a", "user-1");
    let (artifact_id, mut writer) = {
        // 同一管理库上的第一进程：create + append 后「崩溃」（实例被丢弃，
        // finalize/abort 都没跑）。
        let artifacts = ServerArtifacts::open(store.clone(), &root).expect("open");
        let (id, writer) = artifacts.create(&context, spec()).await.expect("create");
        (id, writer)
    };

    // 重启：同一管理库、同一受控目录的打开期扫描把 writing 标记 truncated。
    let artifacts = ServerArtifacts::open(store.clone(), &root).expect("reopen");
    let meta = artifacts
        .describe(&context, artifact_id.clone())
        .await
        .expect("describe after recovery");
    assert_eq!(meta.lifecycle, ArtifactLifecycle::Truncated);
    assert_eq!(
        meta.truncation_reason,
        Some(datazen::execution::domain::AbortReason::WriterLost)
    );

    // 崩溃窗口的孤儿块：正常产物补一个超出已发布数的块文件，重启扫描必须
    // 移除它（恢复后的旧 writer 已作废——writer 能力不跨重启，所以孤儿块
    // 用文件系统手工放置模拟「写到一半进程没了」）。
    let (second_id, mut second_writer) = artifacts
        .create(&context, spec())
        .await
        .expect("create second");
    artifacts
        .append_chunk(&mut second_writer, ChunkPayload::new(vec![1, 2]))
        .await
        .expect("append second");
    let orphan = root.join(second_id.as_str()).join("9.chunk");
    fs::write(&orphan, b"junk").expect("orphan chunk");
    let artifacts = ServerArtifacts::open(store.clone(), &root).expect("reopen again");
    assert!(!orphan.exists(), "孤儿块文件必须被恢复扫描移除");

    // 无元数据行的目录 → 整体移除（known 工件目录不受影响）。
    let bogus = root.join("not-an-artifact");
    fs::create_dir_all(&bogus).expect("bogus dir");
    let _ = ServerArtifacts::open(store, &root).expect("reopen third");
    assert!(!bogus.exists(), "无元数据行的目录必须被恢复扫描移除");
    assert!(
        root.join(second_id.as_str()).exists(),
        "有元数据行的产物目录不被误删"
    );

    drop(second_writer);
    fs::remove_dir_all(&root).ok();
}
