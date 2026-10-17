use super::*;

struct NoWriteGateway(Option<RestoreRejection>);
#[async_trait::async_trait]
impl RestoreGateway for NoWriteGateway {
    async fn validate(&self, _: &RequestContext, _: &RestorePlan) -> Result<(), RestoreRejection> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &RequestContext,
        _: &RestorePlan,
        _: Arc<dyn RestoreInput>,
        _: &CancelToken,
    ) -> RestoreEvidence {
        RestoreEvidence {
            rejection: self.0,
            terminal: StageTerminal::Failed,
            effects: EffectOutcome::NotStarted,
            progress: Default::default(),
            executions: vec![],
            boundaries: vec![],
        }
    }
}
#[tokio::test]
async fn pre_submission_failures_have_stable_reasons_and_never_claim_execution() {
    for (reason, code) in [
        (
            Some(RestoreRejection::RuntimeUnavailable),
            "restoreRuntimeUnavailable",
        ),
        (
            Some(RestoreRejection::AdmissionRejected),
            "restoreAdmissionRejected",
        ),
        (
            Some(RestoreRejection::TargetMismatch),
            "restoreTargetMismatch",
        ),
        (None, "restoreNotSubmitted"),
    ] {
        let (_dir, store, plan) = input().await;
        let job = RestoreApplyJob::new(
            context("alice"),
            plan.clone(),
            store,
            Arc::new(NoWriteGateway(reason)),
        );
        let outcome = job
            .run_stage(&stage(&job, &plan), &CancelToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.terminal, StageTerminal::Failed);
        assert_eq!(outcome.effect_outcome, EffectOutcome::NotStarted);
        assert!(outcome.execution_ids.is_empty());
        assert_eq!(outcome.error_code, Some(ExecutionErrorCode::HostRejected));
        assert_eq!(job.durable_result(&outcome).unwrap().result_code, code);
    }
}
