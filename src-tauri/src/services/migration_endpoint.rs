//! Desktop session identity binding for portable migration admission rules.
use datazen_feature_core::services::migration_endpoint::{checked_identify, normalized};
pub(crate) use datazen_feature_core::services::migration_endpoint::{identify, EndpointIdentity};
/// Resolve a live endpoint, rejecting missing ownership/configuration before admission.
pub(crate) async fn session_identity(
    manager: &super::ConnectionManager,
    db_session_id: &str,
    scope: Option<(&str, Option<&str>)>,
) -> Result<EndpointIdentity, crate::commands::CommandError> {
    use crate::commands::{CmdExt, CommandError};
    let owner = manager
        .owner_connection_id(db_session_id)
        .await
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            CommandError::Validation("Migration session has no owning connection".into())
        })?;
    let (driver, _) = manager
        .get_session(db_session_id)
        .await
        .cmd_err("migration_endpoint")?;
    let mut config = manager
        .migration_identity_config(db_session_id)
        .await
        .cmd_err("migration_endpoint")?;
    if let Some((database, schema)) = scope {
        config.database = normalized(Some(database))
            .or_else(|| normalized(config.database.as_deref()))
            .map(str::to_owned);
        config.schema = crate::services::metadata_schema(
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
        let (_, source) = test.save_and_connect("source-alias").await;
        let (_, target) = test.save_and_connect("target-alias").await;
        let source_identity = session_identity(&test.state.connection_manager, &source, None)
            .await
            .unwrap();
        let target_identity = session_identity(&test.state.connection_manager, &target, None)
            .await
            .unwrap();
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
        let other = session_identity(
            &test.state.connection_manager,
            &source,
            Some(("another-database", None)),
        )
        .await
        .unwrap();
        endpoints[1].connection_id = other.connection_id;
        endpoints[1].service_key = other.service_key;
        assert!(detect_endpoint_overlap(&endpoints).is_ok());
        assert!(
            session_identity(&test.state.connection_manager, "missing", None)
                .await
                .is_err()
        );
    }
}
