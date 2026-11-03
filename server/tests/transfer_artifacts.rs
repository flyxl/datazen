//! WR-D7 受控上传/下载的行为钉子（假体装配：内存 `AdminStore` + 临时受控
//! 目录，无网络、无路由接线）。
//!
//! 钉住的契约：
//!
//! 1. **超限拒绝**：单块超上限 → `PayloadTooLarge`；累计超配额 →
//!    `QuotaExceeded` 且产物被显式 abort（truncated，不宣称完整）。
//! 2. **hash 篡改拒绝**：finalize 声明的 SHA-256 与实际字节不符 →
//!    `InvalidArgument`，产物 abort；声明一致 → `complete`。
//! 3. **越权下载拒绝**：跨组织读取答「不存在」（CM-05 / 防 ID 枚举）；
//!    路径遍历标识符在进仓储前被零容忍拒绝。
//! 4. **三取法互斥**：`?metadata=1` / `?chunkIndex=` / `?offset=&limit=`
//!    的解析行为与前端 `readArtifactQuery` 一致。

use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use datazen::execution::domain::artifact_repository::{ArtifactRepository, ArtifactSpec};
use datazen::execution::domain::ArtifactLifecycle;
use datazen::shared::clock::FixedClock;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{
    AuthenticationSessionId, ClientInstanceId, ExecutionId, OrganizationId, PrincipalId, RequestId,
};

use datazen_server::storage::{AdminStore, ServerArtifacts, ServerAudit};
use datazen_server::transfer::download::{ArtifactReadMode, DownloadReply, DownloadService};
use datazen_server::transfer::upload::{UploadLimits, UploadService};
use datazen_server::transfer::upload::ensure_artifact_id_is_safe;

fn ctx(org: &str, user: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(user),
        Some(AuthenticationSessionId::new("auth-1")),
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn spec() -> ArtifactSpec {
    ArtifactSpec::new(
        OrganizationId::new("org-a"),
        PrincipalId::new("user-a"),
        ExecutionId::new("exec-1"),
        "conn-1/public/t/public.t".into(),
    )
}

type Harness = (UploadService, DownloadService, Arc<ServerArtifacts>, PathBuf);

fn harness(tag: &str, limits: UploadLimits) -> Harness {
    let root = std::env::temp_dir().join(format!("dz-transfer-art-{tag}-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(AdminStore::open_in_memory().expect("in-memory admin store"));
    store.migrate().expect("migrations apply");
    let artifacts = Arc::new(ServerArtifacts::open(store.clone(), &root).expect("artifact root"));
    let clock = Arc::new(FixedClock::at("2026-01-01T00:00:00.000Z"));
    let audit = Arc::new(ServerAudit::new(store));
    let upload = UploadService::with_limits(artifacts.clone(), audit.clone(), clock.clone(), limits);
    let download = DownloadService::new(artifacts.clone(), audit, clock);
    (upload, download, artifacts, root)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[tokio::test]
async fn oversized_chunk_is_rejected_with_payload_too_large() {
    let (upload, _download, _artifacts, _root) = harness(
        "chunk-limit",
        UploadLimits {
            max_chunk_bytes: 64,
            max_total_bytes: 4096,
        },
    );
    let ctx = ctx("org-a", "user-a");
    let mut session = upload.begin_upload(&ctx, spec()).await.expect("upload opens");

    let error = upload
        .append_upload_chunk(&ctx, &mut session, vec![0u8; 65])
        .await
        .expect_err("a chunk over the per-chunk limit is rejected");
    assert_eq!(error.code, ApiErrorCode::PayloadTooLarge);

    // 合法块照常受理：拒绝点在超限块本身。
    upload
        .append_upload_chunk(&ctx, &mut session, vec![1u8; 64])
        .await
        .expect("a chunk at the limit is accepted");
}

#[tokio::test]
async fn total_quota_breach_rejects_and_truncates_the_artifact() {
    let (upload, download, _artifacts, _root) = harness(
        "total-quota",
        UploadLimits {
            max_chunk_bytes: 64,
            max_total_bytes: 128,
        },
    );
    let ctx = ctx("org-a", "user-a");
    let mut session = upload.begin_upload(&ctx, spec()).await.expect("upload opens");
    upload
        .append_upload_chunk(&ctx, &mut session, vec![1u8; 64])
        .await
        .expect("first block within quota");
    upload
        .append_upload_chunk(&ctx, &mut session, vec![2u8; 64])
        .await
        .expect("second block lands exactly at quota");

    let artifact_id = session.artifact_id.as_str().to_string();
    let error = upload
        .append_upload_chunk(&ctx, &mut session, vec![3u8; 1])
        .await
        .expect_err("one byte over the quota is rejected");
    assert_eq!(error.code, ApiErrorCode::QuotaExceeded);

    // 消费超限 → 显式 abort：产物保留已发布块但标记 truncated。
    let metadata = download
        .read(&ctx, &artifact_id, ArtifactReadMode::Metadata)
        .await
        .expect("metadata stays readable after abort");
    let DownloadReply::Metadata(metadata) = metadata else {
        panic!("metadata mode returns metadata");
    };
    assert_eq!(metadata.lifecycle, ArtifactLifecycle::Truncated);
}

#[tokio::test]
async fn hash_tampering_is_rejected_and_honest_upload_completes() {
    let (upload, download, _artifacts, _root) = harness(
        "hash",
        UploadLimits {
            max_chunk_bytes: 64,
            max_total_bytes: 4096,
        },
    );
    let ctx = ctx("org-a", "user-a");

    // 篡改：声明的摘要属于另一份字节。
    let mut session = upload.begin_upload(&ctx, spec()).await.expect("upload opens");
    let artifact_id = session.artifact_id.as_str().to_string();
    upload
        .append_upload_chunk(&ctx, &mut session, b"honest-bytes".to_vec())
        .await
        .expect("chunk lands");
    let error = upload
        .finalize_upload(&ctx, session, Some(&sha256_hex(b"tampered-bytes")))
        .await
        .expect_err("declared digest must match the actual bytes");
    assert_eq!(error.code, ApiErrorCode::InvalidArgument);

    let metadata = download
        .read(&ctx, &artifact_id, ArtifactReadMode::Metadata)
        .await
        .expect("metadata readable");
    let DownloadReply::Metadata(metadata) = metadata else {
        panic!("metadata mode returns metadata");
    };
    assert_eq!(metadata.lifecycle, ArtifactLifecycle::Truncated);

    // 诚实路径：摘要一致 → complete。
    let mut session = upload.begin_upload(&ctx, spec()).await.expect("upload opens");
    let artifact_id = session.artifact_id.as_str().to_string();
    upload
        .append_upload_chunk(&ctx, &mut session, b"honest-bytes".to_vec())
        .await
        .expect("chunk lands");
    let finalized = upload
        .finalize_upload(&ctx, session, Some(&sha256_hex(b"honest-bytes")))
        .await
        .expect("consistent digest finalizes");
    assert_eq!(finalized.as_str(), artifact_id);

    let metadata = download
        .read(&ctx, &artifact_id, ArtifactReadMode::Metadata)
        .await
        .expect("metadata readable");
    let DownloadReply::Metadata(metadata) = metadata else {
        panic!("metadata mode returns metadata");
    };
    assert_eq!(metadata.lifecycle, ArtifactLifecycle::Complete);
}

#[tokio::test]
async fn cross_org_download_is_not_visible() {
    let (upload, download, _artifacts, _root) = harness(
        "cross-org",
        UploadLimits {
            max_chunk_bytes: 64,
            max_total_bytes: 4096,
        },
    );
    let owner = ctx("org-a", "user-a");
    let outsider = ctx("org-b", "user-b");

    let mut session = upload.begin_upload(&owner, spec()).await.expect("upload opens");
    let artifact_id = session.artifact_id.as_str().to_string();
    upload
        .append_upload_chunk(&owner, &mut session, b"secret".to_vec())
        .await
        .expect("chunk lands");
    upload
        .finalize_upload(&owner, session, Some(&sha256_hex(b"secret")))
        .await
        .expect("honest finalize");

    for mode in [
        ArtifactReadMode::Metadata,
        ArtifactReadMode::Chunk { chunk_index: 0 },
        ArtifactReadMode::Range { offset: 0, limit: 6 },
    ] {
        let error = download
            .read(&outsider, &artifact_id, mode)
            .await
            .expect_err("cross-org reads answer not-found (CM-05)");
        assert_eq!(error.code, ApiErrorCode::NotFound);
    }
}

#[tokio::test]
async fn path_traversal_in_artifact_ids_never_reaches_the_store() {
    let (_upload, download, _artifacts, _root) = harness(
        "traversal",
        UploadLimits::default(),
    );
    let ctx = ctx("org-a", "user-a");
    for hostile in ["../escape", "a/b", "a\\b", "..", "x\0y"] {
        let error = download
            .read(&ctx, hostile, ArtifactReadMode::Metadata)
            .await
            .expect_err("hostile identifiers are rejected before the store");
        assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    }
    assert!(ensure_artifact_id_is_safe("018f3d2a-9c1d-7f4e-b2a1-3c5d6e7f8a90").is_ok());
}

#[tokio::test]
async fn the_three_read_modes_are_mutually_exclusive() {
    // 与前端 readArtifactQuery 抛错条件一致：必须恰好选中一种取法。
    assert!(ArtifactReadMode::from_query(true, None, None, None).is_ok());
    assert!(ArtifactReadMode::from_query(false, Some(0), None, None).is_ok());
    assert!(ArtifactReadMode::from_query(false, None, Some(0), Some(4)).is_ok());
    for (metadata, chunk, offset, limit) in [
        (true, Some(0u64), None, None),
        (true, None, Some(0u64), Some(4u64)),
        (false, Some(0u64), Some(0u64), Some(4u64)),
        (false, None, None, None),
        (false, None, Some(0u64), None), // offset 无 limit 不构成区间
    ] {
        let error = ArtifactReadMode::from_query(metadata, chunk, offset, limit)
            .expect_err("mutually exclusive read modes");
        assert_eq!(error.code, ApiErrorCode::InvalidArgument);
    }
}
