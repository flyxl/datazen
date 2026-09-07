use serde::{Deserialize, Serialize};

use crate::ai::{AiModelProfile, AiProviderConfig, AiSettingsConfig};

use super::{Store, StoreError};

impl Store {
    async fn ensure_ai_config_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.ai_config_loaded {
                return;
            }
        }

        // 1. Try loading as AiSettingsConfig (multi-profile)
        let settings_data = self
            .load_encrypted_json::<AiSettingsConfig>("ai_config.enc")
            .await
            .ok();

        let (settings, active_cfg) = match settings_data {
            Some(mut s) if !s.profiles.is_empty() => {
                let active = s
                    .profiles
                    .iter()
                    .find(|p| p.id == s.active_profile_id)
                    .or_else(|| s.profiles.iter().find(|p| p.is_default))
                    .or_else(|| s.profiles.first())
                    .cloned();
                if let Some(ref a) = active {
                    s.active_profile_id = a.id.clone();
                }
                (Some(s), active.map(|p| AiProviderConfig::from(&p)))
            }
            _ => {
                // 2. Fallback to legacy single AiProviderConfig
                let legacy_data = self
                    .load_encrypted_json::<AiProviderConfig>("ai_config.enc")
                    .await
                    .ok();
                if let Some(ref leg) = legacy_data {
                    let mut prof = AiModelProfile::from(leg);
                    prof.id = "default".into();
                    prof.is_default = true;
                    let s = AiSettingsConfig {
                        active_profile_id: "default".into(),
                        profiles: vec![prof],
                    };
                    (Some(s), legacy_data)
                } else {
                    (None, None)
                }
            }
        };

        let mut cache = self.cache.write().await;
        if cache.ai_config_loaded {
            return;
        }
        cache.ai_settings_config = settings;
        cache.ai_config = active_cfg;
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

    pub async fn get_ai_settings_config(&self) -> AiSettingsConfig {
        self.ensure_ai_config_loaded().await;
        let cache = self.cache.read().await;
        cache.ai_settings_config.clone().unwrap_or_default()
    }

    pub async fn save_ai_settings_config(
        &self,
        config: &AiSettingsConfig,
    ) -> Result<(), StoreError> {
        let active_profile = config
            .profiles
            .iter()
            .find(|p| p.id == config.active_profile_id)
            .or_else(|| config.profiles.iter().find(|p| p.is_default))
            .or_else(|| config.profiles.first())
            .cloned();

        let active_cfg = active_profile.map(|p| AiProviderConfig::from(&p));

        {
            let mut cache = self.cache.write().await;
            cache.ai_settings_config = Some(config.clone());
            cache.ai_config = active_cfg;
            cache.ai_config_loaded = true;
        }
        self.save_encrypted_json("ai_config.enc", config).await
    }

    pub async fn set_active_profile(&self, profile_id: &str) -> Result<(), StoreError> {
        self.ensure_ai_config_loaded().await;
        let mut settings = {
            let cache = self.cache.read().await;
            cache.ai_settings_config.clone().unwrap_or_default()
        };
        if let Some(target) = settings.profiles.iter().find(|p| p.id == profile_id) {
            settings.active_profile_id = target.id.clone();
            self.save_ai_settings_config(&settings).await
        } else {
            Err(StoreError::WriteError(format!(
                "Profile {} not found",
                profile_id
            )))
        }
    }

    pub async fn save_ai_config(&self, config: &AiProviderConfig) -> Result<(), StoreError> {
        self.ensure_ai_config_loaded().await;
        let mut settings = {
            let cache = self.cache.read().await;
            cache.ai_settings_config.clone().unwrap_or_default()
        };
        let mut profile = AiModelProfile::from(config);
        profile.id = "default".into();
        profile.is_default = true;

        if let Some(idx) = settings.profiles.iter().position(|p| p.id == "default") {
            settings.profiles[idx] = profile;
        } else {
            settings.profiles.push(profile);
        }
        settings.active_profile_id = "default".into();

        self.save_ai_settings_config(&settings).await
    }

    pub async fn delete_ai_config(&self) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.ai_config = None;
            cache.ai_settings_config = None;
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
