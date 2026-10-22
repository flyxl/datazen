use super::{DesktopRepository, DesktopRepositoryError};
use crate::data_sync::SyncProfile;

impl DesktopRepository {
    async fn ensure_sync_profiles_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.sync_profiles_loaded {
                return;
            }
        }
        // Parse each record independently. A future-version or malformed
        // profile must not hide otherwise usable profiles in the same file.
        let data = match self
            .load_json_file::<Vec<serde_json::Value>>("sync_profiles.json")
            .await
        {
            Ok(records) => records
                .into_iter()
                .filter_map(|record| serde_json::from_value::<SyncProfile>(record).ok())
                .filter(|profile| profile.validate().is_ok())
                .collect(),
            Err(_) => Vec::new(),
        };
        let mut cache = self.cache.write().await;
        if cache.sync_profiles_loaded {
            return;
        }
        cache.sync_profiles = data;
        cache.sync_profiles_loaded = true;
    }

    pub async fn get_sync_profiles(&self) -> Vec<SyncProfile> {
        self.ensure_sync_profiles_loaded().await;
        self.cache.read().await.sync_profiles.clone()
    }

    pub async fn save_sync_profile(
        &self,
        profile: SyncProfile,
    ) -> Result<(), DesktopRepositoryError> {
        profile
            .validate()
            .map_err(DesktopRepositoryError::ParseError)?;
        self.ensure_sync_profiles_loaded().await;
        {
            let mut cache = self.cache.write().await;
            if let Some(existing) = cache.sync_profiles.iter_mut().find(|p| p.id == profile.id) {
                *existing = profile;
            } else {
                cache.sync_profiles.push(profile);
            }
        }
        let snapshot = self.cache.read().await.sync_profiles.clone();
        self.save_json_file("sync_profiles.json", &snapshot).await
    }

    pub async fn delete_sync_profile(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        self.ensure_sync_profiles_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.sync_profiles.retain(|profile| profile.id != id);
        }
        let snapshot = self.cache.read().await.sync_profiles.clone();
        self.save_json_file("sync_profiles.json", &snapshot).await
    }
}
