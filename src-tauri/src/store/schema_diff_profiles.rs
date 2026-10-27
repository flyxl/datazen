use super::{DesktopRepository, DesktopRepositoryError};
use crate::schema_diff::SchemaDiffProfile;
use datazen_modules::schema_diff::{SchemaDiffProfileRepository, SchemaDiffProfileRepositoryError};
use datazen_modules::shared::scope::OrganizationScope;
use serde_json::Value;

const FILE_NAME: &str = "schema_diff_profiles.enc";

impl DesktopRepository {
    async fn ensure_schema_diff_profiles_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.schema_diff_profiles_loaded {
                return;
            }
        }

        let profiles = self
            .load_schema_diff_profiles_from_disk()
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.schema_diff_profiles_loaded {
            return;
        }
        cache.schema_diff_profiles = profiles;
        cache.schema_diff_profiles_loaded = true;
    }

    async fn load_schema_diff_profiles_from_disk(
        &self,
    ) -> Result<Vec<SchemaDiffProfile>, DesktopRepositoryError> {
        let path = self.data_dir.join(FILE_NAME);
        if !path.exists() {
            return Err(DesktopRepositoryError::ReadError("missing".into()));
        }
        let encrypted = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| DesktopRepositoryError::ReadError(error.to_string()))?;
        let plaintext = self.decrypt(&encrypted)?;
        let value: Value = serde_json::from_str(&plaintext)
            .map_err(|error| DesktopRepositoryError::ParseError(error.to_string()))?;
        let records = value.as_array().ok_or_else(|| {
            DesktopRepositoryError::ParseError("schema diff profiles must be an array".into())
        })?;
        Ok(records
            .iter()
            .filter_map(|record| serde_json::from_value::<SchemaDiffProfile>(record.clone()).ok())
            .filter(|profile| profile.validate().is_ok())
            .collect())
    }

    pub async fn get_schema_diff_profiles(&self) -> Vec<SchemaDiffProfile> {
        self.ensure_schema_diff_profiles_loaded().await;
        self.cache.read().await.schema_diff_profiles.clone()
    }

    pub async fn save_schema_diff_profile(
        &self,
        profile: SchemaDiffProfile,
    ) -> Result<(), DesktopRepositoryError> {
        profile
            .validate()
            .map_err(DesktopRepositoryError::ParseError)?;
        self.ensure_schema_diff_profiles_loaded().await;
        let snapshot = {
            let mut cache = self.cache.write().await;
            if let Some(existing) = cache
                .schema_diff_profiles
                .iter_mut()
                .find(|item| item.id == profile.id)
            {
                *existing = profile;
            } else {
                cache.schema_diff_profiles.push(profile);
            }
            cache.schema_diff_profiles.clone()
        };
        self.save_schema_diff_profiles_to_disk(&snapshot).await
    }

    pub async fn delete_schema_diff_profile(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        self.ensure_schema_diff_profiles_loaded().await;
        let snapshot = {
            let mut cache = self.cache.write().await;
            cache
                .schema_diff_profiles
                .retain(|profile| profile.id != id);
            cache.schema_diff_profiles.clone()
        };
        self.save_schema_diff_profiles_to_disk(&snapshot).await
    }

    async fn save_schema_diff_profiles_to_disk(
        &self,
        profiles: &[SchemaDiffProfile],
    ) -> Result<(), DesktopRepositoryError> {
        let _guard = self.write_lock.lock().await;
        let json = serde_json::to_string(profiles)
            .map_err(|error| DesktopRepositoryError::ParseError(error.to_string()))?;
        let encrypted = self.encrypt(&json)?;
        Self::write_file_atomic(&self.data_dir.join(FILE_NAME), encrypted).await
    }
}

fn repository_error(error: DesktopRepositoryError) -> SchemaDiffProfileRepositoryError {
    match error {
        DesktopRepositoryError::ParseError(detail) => {
            SchemaDiffProfileRepositoryError::Invalid(detail)
        }
        other => SchemaDiffProfileRepositoryError::Unavailable(other.to_string()),
    }
}

/// The desktop encrypted store implements the domain-owned Schema Diff profile port.
#[async_trait::async_trait]
impl SchemaDiffProfileRepository for DesktopRepository {
    async fn list(
        &self,
        scope: &OrganizationScope,
    ) -> Result<
        Vec<datazen_modules::schema_diff::SchemaDiffProfile>,
        SchemaDiffProfileRepositoryError,
    > {
        if !crate::platform::is_local_scope(scope) {
            return Ok(Vec::new());
        }
        Ok(self.get_schema_diff_profiles().await)
    }

    async fn save(
        &self,
        scope: &OrganizationScope,
        profile: datazen_modules::schema_diff::SchemaDiffProfile,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(SchemaDiffProfileRepositoryError::NotFound(
                "schema diff profile".into(),
            ));
        }
        self.save_schema_diff_profile(profile)
            .await
            .map_err(repository_error)
    }

    async fn delete(
        &self,
        scope: &OrganizationScope,
        profile_id: &str,
    ) -> Result<(), SchemaDiffProfileRepositoryError> {
        if !crate::platform::is_local_scope(scope) {
            return Err(SchemaDiffProfileRepositoryError::NotFound(
                "schema diff profile".into(),
            ));
        }
        self.delete_schema_diff_profile(profile_id)
            .await
            .map_err(repository_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn profile(id: &str) -> SchemaDiffProfile {
        let now = Utc::now();
        SchemaDiffProfile {
            version: SchemaDiffProfile::CURRENT_VERSION,
            id: id.into(),
            name: "profile".into(),
            source_connection_id: "source".into(),
            target_connection_id: "target".into(),
            source_database: "app".into(),
            target_database: "app".into(),
            source_schema: None,
            target_schema: None,
            target_only_tables: vec![],
            tables: vec!["users".into()],
            source_objects: vec![],
            target_objects: vec![],
            allow_destructive: false,
            include_indexes: true,
            require_rollback: false,
            type_overrides: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn encrypted_round_trip_filters_invalid_records() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("store");
        store
            .save_schema_diff_profile(profile("valid"))
            .await
            .expect("save profile");
        let encrypted = tokio::fs::read_to_string(temp.path().join(FILE_NAME))
            .await
            .expect("encrypted profile file");
        assert!(!encrypted.contains("sourceConnectionId"));
        let plaintext = store.decrypt(&encrypted).expect("decrypt profile file");
        assert!(plaintext.contains("sourceConnectionId"));

        let mut records: Value = serde_json::from_str(&plaintext).expect("records");
        records
            .as_array_mut()
            .expect("array")
            .push(serde_json::json!({"version": 999, "id": "bad"}));
        let encrypted = store
            .encrypt(&serde_json::to_string(&records).expect("records json"))
            .expect("encrypt records");
        tokio::fs::write(temp.path().join(FILE_NAME), encrypted)
            .await
            .expect("write records");
        let fresh = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("reload store");
        assert_eq!(fresh.get_schema_diff_profiles().await.len(), 1);
    }

    #[tokio::test]
    async fn test_tester_round_trip_preserves_scope_options_and_type_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("store");
        let mut expected = profile("with-override");
        expected.source_schema = Some("source_schema".into());
        expected.target_schema = Some("target_schema".into());
        expected.target_only_tables = vec!["target_schema.archive".into()];
        expected.allow_destructive = true;
        expected.include_indexes = false;
        expected.require_rollback = true;
        expected.source_objects = vec![datazen_driver_api::schema_objects::DatabaseObject {
            kind: "function".into(),
            schema: Some("source_schema".into()),
            name: "calculate_total".into(),
            signature: Some("integer, numeric".into()),
            target_schema: None,
            target_name: None,
        }];
        expected.target_objects = vec![datazen_driver_api::schema_objects::DatabaseObject {
            kind: "trigger".into(),
            schema: Some("target_schema".into()),
            name: "audit_orders".into(),
            signature: None,
            target_schema: Some("target_schema".into()),
            target_name: Some("orders".into()),
        }];
        expected.type_overrides = vec![crate::schema_diff::types::ColumnTypeOverride {
            table: "source_schema.users".into(),
            column: "name".into(),
            target_type: "VARCHAR(64)".into(),
        }];

        store
            .save_schema_diff_profile(expected.clone())
            .await
            .expect("save profile");
        let fresh = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("reload store");
        let actual = fresh
            .get_schema_diff_profiles()
            .await
            .into_iter()
            .find(|item| item.id == expected.id)
            .expect("profile survives reload");
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn domain_profile_port_round_trips_and_deletes_in_the_local_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("store");
        let expected = profile("port-round-trip");
        let scope = crate::platform::local_organization_scope();

        SchemaDiffProfileRepository::save(&store, &scope, expected.clone())
            .await
            .expect("save through domain port");
        assert_eq!(
            SchemaDiffProfileRepository::list(&store, &scope)
                .await
                .expect("list through domain port"),
            vec![expected]
        );

        SchemaDiffProfileRepository::delete(&store, &scope, "port-round-trip")
            .await
            .expect("delete through domain port");
        assert!(SchemaDiffProfileRepository::list(&store, &scope)
            .await
            .expect("list after delete through domain port")
            .is_empty());
    }

    #[tokio::test]
    async fn domain_profile_port_fails_closed_for_a_foreign_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path(), crate::store::KeyBackend::File)
            .await
            .expect("store");
        let foreign = datazen_modules::shared::scope::OrganizationScope::new(
            datazen_modules::shared::id::OrganizationId::new("foreign-org"),
        );

        assert!(SchemaDiffProfileRepository::list(&store, &foreign)
            .await
            .expect("foreign reads are treated as empty")
            .is_empty());
        assert!(matches!(
            SchemaDiffProfileRepository::save(&store, &foreign, profile("foreign")).await,
            Err(SchemaDiffProfileRepositoryError::NotFound(_))
        ));
    }
}
