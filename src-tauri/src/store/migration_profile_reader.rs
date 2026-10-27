//! Desktop implementation of the shared migration-profile read projection.
//!
//! The history command only needs a stable profile id and revision. It must not
//! know the concrete Data Sync / Data Transfer / Schema Diff persistence files,
//! so this module adapts the three desktop stores to the narrow platform port.

use datazen_modules::application::migration_profile::{
    MigrationProfileReader, MigrationProfileSummary,
};
use datazen_modules::shared::context::RequestContext;
use datazen_modules::shared::id::{ConnectionId, Timestamp};
use datazen_modules::shared::scope::OrganizationScope;
use datazen_modules::PortError;

use super::DesktopRepository;

fn summary(
    id: String,
    name: String,
    source_connection_id: String,
    target_connection_id: Option<String>,
    source_database: Option<String>,
    target_database: Option<String>,
    source_schema: Option<String>,
    target_schema: Option<String>,
    updated_at: chrono::DateTime<chrono::Utc>,
) -> MigrationProfileSummary {
    MigrationProfileSummary {
        id,
        name,
        source_connection_id: ConnectionId::new(source_connection_id),
        target_connection_id: target_connection_id.map(ConnectionId::new),
        source_database,
        target_database,
        source_schema,
        target_schema,
        updated_at: Timestamp::new(updated_at.to_rfc3339()),
    }
}

fn scope_for(context: &RequestContext) -> OrganizationScope {
    OrganizationScope::from_context(context)
}

fn ensure_local_scope(scope: &OrganizationScope) -> bool {
    crate::platform::is_local_scope(scope)
}

#[async_trait::async_trait]
impl MigrationProfileReader for DesktopRepository {
    async fn list_transfer_profiles(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError> {
        if !ensure_local_scope(&scope_for(context)) {
            return Ok(Vec::new());
        }
        Ok(self
            .get_transfer_profiles()
            .await
            .into_iter()
            .map(|profile| {
                summary(
                    profile.id,
                    profile.name,
                    profile.source_connection_id,
                    profile.target_connection_id,
                    profile.source_database,
                    profile.target_database,
                    profile.source_schema,
                    profile.target_schema,
                    profile.updated_at,
                )
            })
            .collect())
    }

    async fn list_sync_profiles(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError> {
        if !ensure_local_scope(&scope_for(context)) {
            return Ok(Vec::new());
        }
        Ok(self
            .get_sync_profiles()
            .await
            .into_iter()
            .map(|profile| {
                summary(
                    profile.id,
                    profile.name,
                    profile.source_connection_id,
                    Some(profile.target_connection_id),
                    profile.source_database,
                    profile.target_database,
                    profile.source_schema,
                    profile.target_schema,
                    profile.updated_at,
                )
            })
            .collect())
    }

    async fn list_schema_diff_profiles(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<MigrationProfileSummary>, PortError> {
        if !ensure_local_scope(&scope_for(context)) {
            return Ok(Vec::new());
        }
        Ok(self
            .get_schema_diff_profiles()
            .await
            .into_iter()
            .map(|profile| {
                summary(
                    profile.id,
                    profile.name,
                    profile.source_connection_id,
                    Some(profile.target_connection_id),
                    Some(profile.source_database),
                    Some(profile.target_database),
                    profile.source_schema,
                    profile.target_schema,
                    profile.updated_at,
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_modules::shared::id::OrganizationId;

    #[tokio::test]
    async fn the_desktop_reader_returns_empty_for_local_and_foreign_scopes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path(), super::super::KeyBackend::File)
            .await
            .expect("store");
        let adapter = crate::platform::identity::DesktopIdentity::new_launch(
            crate::platform::identity::LOCAL_ORGANIZATION_ID,
            "test-user",
            "test-client",
        )
        .expect("identity");
        let local = adapter.request_context(datazen_modules::shared::id::RequestId::new("request"));
        let foreign = RequestContext::new(
            OrganizationId::new("foreign-org"),
            datazen_modules::shared::id::PrincipalId::new("test-user"),
            None,
            datazen_modules::shared::id::ClientInstanceId::new("test-client"),
            datazen_modules::shared::id::RequestId::new("foreign-request"),
            None,
        );

        assert!(store
            .list_sync_profiles(&local)
            .await
            .expect("local read")
            .is_empty());
        assert!(store
            .list_transfer_profiles(&local)
            .await
            .expect("local read")
            .is_empty());
        assert!(store
            .list_schema_diff_profiles(&local)
            .await
            .expect("local read")
            .is_empty());
        assert!(store
            .list_sync_profiles(&foreign)
            .await
            .expect("foreign read")
            .is_empty());
    }
}
