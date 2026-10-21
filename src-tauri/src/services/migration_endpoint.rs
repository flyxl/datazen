//! Desktop session identity binding for portable migration admission rules.
use datazen_feature_core::services::migration_endpoint::{checked_identify, normalized};
pub(crate) use datazen_feature_core::services::migration_endpoint::{identify, EndpointIdentity};

pub(crate) struct RuntimeSessionContext {
    pub(crate) view: datazen_platform_api::dto::session::SessionView,
    pub(crate) driver: std::sync::Arc<dyn datazen_driver_api::DatabaseDriver>,
    pub(crate) handle: datazen_driver_api::ConnectionHandle,
    pub(crate) config: datazen_driver_api::ConnectionConfig,
    pub(crate) _lease: datazen_runtime::application::LiveConnectionLease,
}

pub(crate) async fn runtime_session_context(
    state: &crate::commands::AppState,
    db_session_id: &str,
    purpose: datazen_runtime::application::LivePurpose,
) -> Result<RuntimeSessionContext, crate::commands::CommandError> {
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let (view, lease) = runtime
        .borrow_live_connection_by_id(call.context(), db_session_id, purpose)
        .await
        .map_err(crate::platform::into_command_error)?;
    let connection = lease
        .connection()
        .map_err(|error| crate::commands::CommandError::NotFound(error.to_string()))?;
    let mut config = state
        .store
        .get_connection(view.connection_id.as_str())
        .await
        .ok_or_else(|| {
            crate::commands::CommandError::NotFound("connection profile is unavailable".into())
        })?;
    config.database = view
        .observed_context
        .namespace
        .database
        .clone()
        .or(config.database);
    config.schema = view
        .observed_context
        .namespace
        .schema
        .clone()
        .or(config.schema);
    Ok(RuntimeSessionContext {
        view,
        driver: std::sync::Arc::clone(connection.driver()),
        handle: connection.handle().clone(),
        config,
        _lease: lease,
    })
}

pub(crate) async fn open_dedicated_session(
    state: &crate::commands::AppState,
    connection_id: &str,
    database: &str,
    purpose: &str,
) -> Result<String, crate::commands::CommandError> {
    use datazen_application::{dto::requests::OpenSessionRequest, sessions::ConnectionUseCases};
    use datazen_platform_api::{
        context::OwnerRef,
        dto::idempotency::IdempotentOperation,
        id::ConnectionId,
        target::{ExecutionTarget, NamespaceTarget},
    };
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let token = runtime
        .issue_submission_token(call.context(), IdempotentOperation::OpenSession, None)
        .await
        .map_err(crate::platform::into_command_error)?;
    let receipt = runtime
        .open_session(
            call.context(),
            OpenSessionRequest {
                initial_target: ExecutionTarget::new(
                    ConnectionId::new(connection_id),
                    NamespaceTarget::new(Some(database.to_owned()), None, None, Vec::new()),
                ),
                owner: OwnerRef::ClientSession {
                    client_instance_id: call.context().client_instance_id.clone(),
                    purpose: purpose.to_owned(),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .map_err(crate::platform::into_command_error)?;
    Ok(receipt.session.handle.db_session_id.as_str().to_owned())
}

pub(crate) async fn close_dedicated_session(
    state: &crate::commands::AppState,
    db_session_id: &str,
) -> Result<(), crate::commands::CommandError> {
    use datazen_application::{
        dto::requests::{CloseMode, CloseSessionRequest},
        sessions::ConnectionUseCases,
    };
    let adapter = state
        .platform_require()
        .map_err(crate::platform::into_command_error)?;
    let call = adapter.begin_call();
    let runtime = adapter
        .runtime()
        .map_err(crate::platform::into_command_error)?;
    let view = runtime
        .get_session_by_id(call.context(), db_session_id)
        .await
        .map_err(crate::platform::into_command_error)?;
    runtime
        .close_session(
            call.context(),
            CloseSessionRequest {
                handle: view.handle,
                mode: CloseMode::RollbackAndClose,
            },
        )
        .await
        .map(|_| ())
        .map_err(crate::platform::into_command_error)
}
/// Resolve a live endpoint, rejecting missing ownership/configuration before admission.
pub(crate) async fn session_identity(
    state: &crate::commands::AppState,
    db_session_id: &str,
    scope: Option<(&str, Option<&str>)>,
) -> Result<EndpointIdentity, crate::commands::CommandError> {
    let context = runtime_session_context(
        state,
        db_session_id,
        datazen_runtime::application::LivePurpose::Inspection,
    )
    .await?;
    let view = context.view;
    let owner = view.connection_id.as_str().to_owned();
    let driver = &context.driver;
    let mut config = context.config;
    config.database = view
        .observed_context
        .namespace
        .database
        .clone()
        .or(config.database);
    config.schema = view
        .observed_context
        .namespace
        .schema
        .clone()
        .or(config.schema);
    if let Some((database, schema)) = scope {
        config.database = normalized(Some(database))
            .or_else(|| normalized(config.database.as_deref()))
            .map(str::to_owned);
        config.schema = datazen_feature_core::services::schema_scope::metadata_schema(
            driver.as_ref(),
            schema,
            None,
            config.schema.as_deref(),
        );
    }
    config.id = owner;
    checked_identify(&config, driver.as_ref()).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn session_aliases_are_refused_and_cross_database_sessions_remain_legal() {
        use crate::testing::app_state::TestAppState;
        use datazen_runtime::job::{detect_endpoint_overlap, EndpointRef, EndpointRole};
        let test = TestAppState::new().await;
        let (_, source) = test.save_and_connect_runtime("source-alias").await;
        let (_, target) = test.save_and_connect_runtime("target-alias").await;
        let source_identity = session_identity(&test.state, &source, None).await.unwrap();
        let target_identity = session_identity(&test.state, &target, None).await.unwrap();
        let mut endpoints = vec![
            EndpointRef {
                connection_id: source_identity.connection_id,
                service_key: source_identity.service_key,
                objects: vec!["users".into()],
                role: EndpointRole::SourceReader,
            },
            EndpointRef {
                connection_id: target_identity.connection_id,
                service_key: target_identity.service_key,
                objects: vec!["users".into()],
                role: EndpointRole::TargetWriter,
            },
        ];
        assert!(detect_endpoint_overlap(&endpoints).is_err());
        let other = session_identity(&test.state, &source, Some(("another-database", None)))
            .await
            .unwrap();
        endpoints[1].connection_id = other.connection_id;
        endpoints[1].service_key = other.service_key;
        assert!(detect_endpoint_overlap(&endpoints).is_ok());
        assert!(session_identity(&test.state, "missing", None)
            .await
            .is_err());
    }
}
