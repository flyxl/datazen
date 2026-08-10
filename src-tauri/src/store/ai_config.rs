use serde::{Deserialize, Serialize};

use crate::ai::AiProviderConfig;

use super::{Store, StoreError};

impl Store {
    async fn ensure_ai_config_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.ai_config_loaded {
                return;
            }
        }
        let data = self
            .load_encrypted_json::<AiProviderConfig>("ai_config.enc")
            .await
            .ok();
        let mut cache = self.cache.write().await;
        if cache.ai_config_loaded {
            return;
        }
        cache.ai_config = data;
        cache.ai_config_loaded = true;
        tracing::debug!(
            present = cache.ai_config.is_some(),
            "Loaded AI config on demand"
        );
    }

    pub async fn get_ai_config(&self) -> Option<AiProviderConfig> {
        self.ensure_ai_config_loaded().await;
        let cache = self.cache.read().await;
        cache.ai_config.clone()
    }

    pub async fn save_ai_config(&self, config: &AiProviderConfig) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.ai_config = Some(config.clone());
            cache.ai_config_loaded = true;
        }
        self.save_encrypted_json("ai_config.enc", config).await
    }

    pub async fn delete_ai_config(&self) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.ai_config = None;
            cache.ai_config_loaded = true;
        }
        let path = self.data_dir.join("ai_config.enc");
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| StoreError::WriteError(e.to_string()))?;
        }
        Ok(())
    }

    async fn load_encrypted_json<T>(&self, filename: &str) -> Result<T, StoreError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.data_dir.join(filename);
        if !path.exists() {
            return Err(StoreError::ReadError("missing".into()));
        }
        let encrypted = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;
        let plaintext = self.decrypt(&encrypted)?;
        serde_json::from_str(&plaintext).map_err(|e| StoreError::ParseError(e.to_string()))
    }

    async fn save_encrypted_json<T: Serialize>(
        &self,
        filename: &str,
        data: &T,
    ) -> Result<(), StoreError> {
        let path = self.data_dir.join(filename);
        let json =
            serde_json::to_string(data).map_err(|e| StoreError::ParseError(e.to_string()))?;
        let encrypted = self.encrypt(&json)?;
        Self::write_file_atomic(&path, encrypted).await
    }
}
