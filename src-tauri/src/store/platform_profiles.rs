use super::{Store, StoreError};
use crate::db::ConnectionConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMetadata {
    pub config_revision: u64,
    pub credential_revision: u64,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub creation_key: Option<String>,
}
impl Default for ProfileMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            config_revision: 1,
            credential_revision: 1,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
            creation_key: None,
        }
    }
}

pub(super) fn encode_records(
    configs: Vec<ConnectionConfig>,
    metadata: &BTreeMap<String, ProfileMetadata>,
) -> Result<Vec<serde_json::Value>, StoreError> {
    configs
        .into_iter()
        .map(|config| {
            let id = config.id.clone();
            let mut value = serde_json::to_value(config)
                .map_err(|_| StoreError::ParseError("connection serialization failed".into()))?;
            value["platformProfile"] =
                serde_json::to_value(metadata.get(&id).cloned().unwrap_or_default())
                    .map_err(|_| StoreError::ParseError("profile serialization failed".into()))?;
            Ok(value)
        })
        .collect()
}
impl Store {
    pub(super) async fn load_platform_metadata(
        &self,
    ) -> Result<BTreeMap<String, ProfileMetadata>, StoreError> {
        let path = self.data_dir.join("connections.json");
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|_| StoreError::ReadError("profile read failed".into()))?;
        let records: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
            .map_err(|_| StoreError::ParseError("profile parse failed".into()))?;
        records
            .into_iter()
            .map(|record| {
                let id = record["id"]
                    .as_str()
                    .ok_or_else(|| StoreError::ParseError("connection id missing".into()))?
                    .to_owned();
                let metadata = match record.get("platformProfile") {
                    Some(value) => serde_json::from_value(value.clone())
                        .map_err(|_| StoreError::ParseError("profile metadata invalid".into()))?,
                    None => ProfileMetadata::default(),
                };
                Ok((id, metadata))
            })
            .collect()
    }
    pub async fn platform_profile(&self, id: &str) -> Option<(ConnectionConfig, ProfileMetadata)> {
        let cache = self.cache.read().await;
        let config = cache.connections.iter().find(|c| c.id == id)?.clone();
        Some((
            config,
            cache.platform_profiles.get(id).cloned().unwrap_or_default(),
        ))
    }
    pub async fn platform_creation(
        &self,
        key: &str,
    ) -> Option<(ConnectionConfig, ProfileMetadata)> {
        let cache = self.cache.read().await;
        let (id, metadata) = cache
            .platform_profiles
            .iter()
            .find(|(_, m)| m.creation_key.as_deref() == Some(key))?;
        Some((
            cache.connections.iter().find(|c| &c.id == id)?.clone(),
            metadata.clone(),
        ))
    }
    pub async fn platform_update<F>(
        &self,
        id: &str,
        expected: Option<u64>,
        mutate: F,
    ) -> Result<(ConnectionConfig, ProfileMetadata), datazen_platform_api::error::PortError>
    where
        F: FnOnce(
                Option<ConnectionConfig>,
                &mut ProfileMetadata,
            ) -> Result<ConnectionConfig, datazen_platform_api::error::PortError>
            + Send,
    {
        use datazen_platform_api::error::PortError;
        let _guard = self.write_lock.lock().await;
        let mut cache = self.cache.write().await;
        let previous = cache.connections.iter().find(|c| c.id == id).cloned();
        let previous_metadata = cache.platform_profiles.get(id).cloned();
        let mut metadata = previous_metadata.clone().unwrap_or_default();
        if expected
            .is_some_and(|revision| previous.is_none() || metadata.config_revision != revision)
        {
            return Err(PortError::CasConflict {
                entity: "profile",
                id: id.into(),
            });
        }
        let config = mutate(previous.clone(), &mut metadata)?;
        metadata.updated_at = chrono::Utc::now().to_rfc3339();
        cache.connections.retain(|c| c.id != id);
        cache.connections.push(config.clone());
        cache.platform_profiles.insert(id.into(), metadata.clone());
        let snapshot = cache.connections.clone();
        drop(cache);
        if self.persist_connections_locked(&snapshot).await.is_err() {
            let mut cache = self.cache.write().await;
            cache.connections.retain(|c| c.id != id);
            if let Some(previous) = previous {
                cache.connections.push(previous);
            }
            match previous_metadata {
                Some(previous) => {
                    cache.platform_profiles.insert(id.into(), previous);
                }
                None => {
                    cache.platform_profiles.remove(id);
                }
            }
            return Err(PortError::BackendUnavailable(
                "profile persistence failed".into(),
            ));
        }
        Ok((config, metadata))
    }
}
