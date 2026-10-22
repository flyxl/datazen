//! Desktop policy binding for the shared Runtime restore orchestrator.
use datazen_feature_core::{
    restore_gateway::{RestoreAuthorization, RuntimeRestoreGateway},
    restore_job::*,
};
use datazen_platform_api::RequestContext;
use datazen_runtime::job::CancelToken;
use std::sync::Arc;

pub(super) struct DesktopRestoreGateway {
    pub adapter: Arc<crate::platform::PlatformAdapter>,
    pub store: Arc<crate::store::DesktopRepository>,
    pub driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    pub profile: String,
}
struct Authorization {
    adapter: Arc<crate::platform::PlatformAdapter>,
    store: Arc<crate::store::DesktopRepository>,
}
#[async_trait::async_trait]
impl RestoreAuthorization for Authorization {
    async fn validate(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection> {
        let (config, metadata) = self
            .store
            .platform_profile(plan.target.connection_id.as_str())
            .await
            .ok_or(RestoreRejection::NotAuthorized)?;
        if !metadata.enabled
            || metadata.config_revision != plan.config_revision.get()
            || config.read_only
            || self.store.execution_settings().await.safe_mode
        {
            return Err(RestoreRejection::NotAuthorized);
        }
        self.adapter
            .resolve_execution_target(ctx, plan.config_revision, "execute", &plan.target)
            .await
            .map_err(|_| RestoreRejection::NotAuthorized)?;
        Ok(())
    }
}
#[async_trait::async_trait]
impl RestoreGateway for DesktopRestoreGateway {
    async fn validate(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
    ) -> Result<(), RestoreRejection> {
        if !matches!(
            self.profile.as_str(),
            datazen_driver_api::restore_sql::TRANSACTIONAL
                | datazen_driver_api::restore_sql::IMPLICIT_COMMIT
        ) {
            return Err(RestoreRejection::Unsupported);
        }
        Authorization {
            adapter: self.adapter.clone(),
            store: self.store.clone(),
        }
        .validate(ctx, plan)
        .await
    }
    async fn execute(
        &self,
        ctx: &RequestContext,
        plan: &RestorePlan,
        input: Arc<dyn RestoreInput>,
        cancel: &CancelToken,
    ) -> RestoreEvidence {
        let Ok(runtime) = self.adapter.runtime() else {
            return RestoreEvidence {
                rejection: Some(RestoreRejection::RuntimeUnavailable),
                terminal: datazen_runtime::job::StageTerminal::Failed,
                effects: datazen_platform_api::dto::execution::EffectOutcome::NotStarted,
                progress: Default::default(),
                executions: vec![],
                boundaries: vec![],
            };
        };
        RuntimeRestoreGateway {
            runtime,
            authorization: Arc::new(Authorization {
                adapter: self.adapter.clone(),
                store: self.store.clone(),
            }),
            driver: self.driver.clone(),
            profile: self.profile.clone(),
        }
        .execute(ctx, plan, input, cancel)
        .await
    }
}
