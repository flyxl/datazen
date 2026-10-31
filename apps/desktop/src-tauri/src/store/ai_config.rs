use serde::{Deserialize, Serialize};

use datazen::ai::application::AiUseCaseError;
use datazen::ai::{AiModelProfile, AiProviderConfig, AiSettingsConfig};

use super::{DesktopRepository, DesktopRepositoryError};

impl DesktopRepository {
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
                let active = s.selected_profile().cloned();
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
    ) -> Result<(), DesktopRepositoryError> {
        let active_cfg = config.selected_profile().map(AiProviderConfig::from);

        {
            let mut cache = self.cache.write().await;
            cache.ai_settings_config = Some(config.clone());
            cache.ai_config = active_cfg;
            cache.ai_config_loaded = true;
        }
        self.save_encrypted_json("ai_config.enc", config).await
    }

    pub async fn set_active_profile(&self, profile_id: &str) -> Result<(), DesktopRepositoryError> {
        self.ensure_ai_config_loaded().await;
        let mut settings = {
            let cache = self.cache.read().await;
            cache.ai_settings_config.clone().unwrap_or_default()
        };
        if let Some(target) = settings.profiles.iter().find(|p| p.id == profile_id) {
            settings.active_profile_id = target.id.clone();
            self.save_ai_settings_config(&settings).await
        } else {
            Err(DesktopRepositoryError::WriteError(format!(
                "Profile {} not found",
                profile_id
            )))
        }
    }

    pub async fn save_ai_config(
        &self,
        config: &AiProviderConfig,
    ) -> Result<(), DesktopRepositoryError> {
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

    pub async fn delete_ai_config(&self) -> Result<(), DesktopRepositoryError> {
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
                .map_err(|e| DesktopRepositoryError::WriteError(e.to_string()))?;
        }
        Ok(())
    }

    async fn load_encrypted_json<T>(&self, filename: &str) -> Result<T, DesktopRepositoryError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.data_dir.join(filename);
        if !path.exists() {
            return Err(DesktopRepositoryError::ReadError("missing".into()));
        }
        let encrypted = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(e.to_string()))?;
        let plaintext = self.decrypt(&encrypted)?;
        serde_json::from_str(&plaintext)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))
    }

    async fn save_encrypted_json<T: Serialize>(
        &self,
        filename: &str,
        data: &T,
    ) -> Result<(), DesktopRepositoryError> {
        let path = self.data_dir.join(filename);
        let json = serde_json::to_string(data)
            .map_err(|e| DesktopRepositoryError::ParseError(e.to_string()))?;
        let encrypted = self.encrypt(&json)?;
        Self::write_file_atomic(&path, encrypted).await
    }
}

/// AI 配置族持久化端口实现（host-boundary §3，AI 配置刀）。
///
/// 端口契约在 `datazen::ai::application::ports::AiConfigStore`；桌面把
/// 「`{appData}/ai_config.enc` 加密单文件 + 内存缓存」这一事实接到端口上。
/// 存储错误统一折成用例信封的 `Internal` 桶（Display 文本原样透传，经桌面
/// 映射表后 IPC 文案与旧 `CommandError::Store` 路径逐字节一致——两者都只
/// 打印错误文本）。语言与执行设置是宿主设置事实的派生读量，供提示词解析
/// 与安全门用例消费。
#[async_trait::async_trait]
impl datazen::ai::application::ports::AiConfigStore for DesktopRepository {
    async fn get_ai_config(&self) -> Option<AiProviderConfig> {
        DesktopRepository::get_ai_config(self).await
    }

    async fn get_ai_settings_config(&self) -> AiSettingsConfig {
        DesktopRepository::get_ai_settings_config(self).await
    }

    async fn save_ai_settings_config(
        &self,
        config: &AiSettingsConfig,
    ) -> Result<(), AiUseCaseError> {
        DesktopRepository::save_ai_settings_config(self, config)
            .await
            .map_err(|e| AiUseCaseError::Internal(e.to_string()))
    }

    async fn set_active_profile(&self, profile_id: &str) -> Result<(), AiUseCaseError> {
        DesktopRepository::set_active_profile(self, profile_id)
            .await
            .map_err(|e| AiUseCaseError::Internal(e.to_string()))
    }

    async fn save_ai_config(&self, config: &AiProviderConfig) -> Result<(), AiUseCaseError> {
        DesktopRepository::save_ai_config(self, config)
            .await
            .map_err(|e| AiUseCaseError::Internal(e.to_string()))
    }

    async fn delete_ai_config(&self) -> Result<(), AiUseCaseError> {
        DesktopRepository::delete_ai_config(self)
            .await
            .map_err(|e| AiUseCaseError::Internal(e.to_string()))
    }

    async fn settings_language(&self) -> String {
        DesktopRepository::get_settings(self).await.language
    }

    async fn execution_settings(&self) -> datazen::settings::ExecutionSettings {
        DesktopRepository::execution_settings(self).await
    }
}
