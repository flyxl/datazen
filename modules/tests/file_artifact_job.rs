//! File artifact job 行为用例（D6 item 5 切片 a 随迁 feature-core 集成测试）。
//!
//! 唯一定义在 `datazen::export::file_job`；这些用例需要一个真实
//! `FileArtifactRepository`（runtime dev-dep），故留在 feature-core 垫片层
//! ——与 D5 data_sync / D6 schema_context 垫片保留集成测试同一先例，
//! D8 退役 feature-core 时随垫片一并处置。

#[cfg(test)]
mod tests {
    use datazen::execution::runtime::artifact::FileArtifactRepository;
    use datazen::export::ExportError as FeatureError;
    use datazen::file_job::{FileArtifactJob, FileJobOperation, FileJobOutput};
    use datazen::jobs::{CancelToken, JobHandler, StageSpec, StageTerminal};
    use datazen::shared::id::StageId;
    use datazen::{
        execution::domain::ArtifactRepository,
        execution::ArtifactLifecycle,
        shared::context::RequestContext,
        shared::id::{ClientInstanceId, Counter, OrganizationId, PrincipalId, RequestId},
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    struct Output {
        path: PathBuf,
    }
    #[async_trait::async_trait]
    impl FileJobOperation for Output {
        async fn run(&self, _: &CancelToken) -> Result<FileJobOutput, FeatureError> {
            Ok(FileJobOutput {
                path: self.path.clone(),
                rows: 7,
            })
        }
    }
    fn context(principal: &str) -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org"),
            PrincipalId::new(principal),
            None,
            ClientInstanceId::new("client"),
            RequestId::new("request"),
            None,
        )
    }
    fn stage() -> StageSpec {
        StageSpec {
            stage_id: StageId::new("produceArtifact"),
            kind: "tableExport".into(),
            depends_on: vec![],
        }
    }
    #[tokio::test]
    async fn output_is_finalized_raw_artifact_owned_by_job_principal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("staged.csv");
        tokio::fs::write(&path, b"id,name\n1,alice\n")
            .await
            .unwrap();
        let store = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
        let job = FileArtifactJob::new(
            "tableExport",
            context("alice"),
            store.clone(),
            Arc::new(Output { path }),
        );
        let result = job.run_stage(&stage(), &CancelToken::new()).await.unwrap();
        assert_eq!(result.terminal, StageTerminal::Succeeded);
        let (artifact, rows) = job.output().unwrap();
        assert_eq!(rows, 7);
        assert_eq!(result.artifact_ids, vec![artifact.clone()]);
        assert_eq!(
            store
                .describe(&context("alice"), artifact.clone())
                .await
                .unwrap()
                .lifecycle,
            ArtifactLifecycle::Complete
        );
        assert_eq!(
            store
                .read_chunk(&context("alice"), artifact.clone(), Counter::ZERO)
                .await
                .unwrap()
                .bytes,
            b"id,name\n1,alice\n"
        );
        assert!(store.describe(&context("bob"), artifact).await.is_err());
    }
    #[tokio::test]
    async fn cancel_before_dispatch_does_not_open_or_publish_source() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
        let job = FileArtifactJob::new(
            "tableExport",
            context("alice"),
            store,
            Arc::new(Output {
                path: dir.path().join("missing"),
            }),
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        let result = job.run_stage(&stage(), &cancel).await.unwrap();
        assert_eq!(result.terminal, StageTerminal::Cancelled);
        assert!(result.artifact_ids.is_empty());
        assert!(job.output().is_none());
    }
    #[tokio::test]
    async fn producer_failure_does_not_claim_a_complete_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
        let job = FileArtifactJob::new(
            "tableExport",
            context("alice"),
            store,
            Arc::new(Output {
                path: dir.path().join("missing"),
            }),
        );
        let result = job.run_stage(&stage(), &CancelToken::new()).await.unwrap();
        assert_eq!(result.terminal, StageTerminal::Failed);
        assert!(result.artifact_ids.is_empty());
        assert!(job.output().is_none());
    }
}
