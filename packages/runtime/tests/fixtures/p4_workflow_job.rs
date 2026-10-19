//! Caller-supplied owner fields never grant Runtime session authority.
//! Workflow block execution fixtures were retired with the P6 v1-only scope;
//! the negative Job/WorkflowBlock owner boundary remains applicable.
use super::*;

#[tokio::test]
async fn caller_owner_fields_cannot_authorize_job_or_block() {
    let (service, backend, _, _) = setup();
    let context = ctx("user", "desktop");
    for owner in [
        OwnerRef::Job {
            job_id: JobId::new("invented"),
            stage_id: StageId::new("workflow"),
        },
        OwnerRef::WorkflowBlock {
            job_id: JobId::new("invented"),
            block_id: BlockId::new("block"),
        },
    ] {
        let token = service
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .unwrap();
        assert!(service
            .open_session(
                &context,
                OpenSessionRequest::new(target("a"), owner, token.idempotency_key)
            )
            .await
            .is_err());
    }
    assert_eq!(backend.opened.load(Ordering::SeqCst), 0);
}
