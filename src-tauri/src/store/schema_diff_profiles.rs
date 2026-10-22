use super::{DesktopRepository, DesktopRepositoryError};
use crate::schema_diff::SchemaDiffProfile;
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
        let _keyring = crate::testing::FileKeyringGuard::set();
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path())
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
        let fresh = DesktopRepository::init_with_path(temp.path())
            .await
            .expect("reload store");
        assert_eq!(fresh.get_schema_diff_profiles().await.len(), 1);
    }

    #[tokio::test]
    async fn test_tester_round_trip_preserves_scope_options_and_type_overrides() {
        let _keyring = crate::testing::FileKeyringGuard::set();
        let temp = tempfile::tempdir().expect("tempdir");
        let store = DesktopRepository::init_with_path(temp.path())
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
        let fresh = DesktopRepository::init_with_path(temp.path())
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
}
