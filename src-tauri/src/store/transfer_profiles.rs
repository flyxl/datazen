use super::{DesktopRepository, DesktopRepositoryError};
use crate::data_transfer::TransferProfile;

impl DesktopRepository {
    async fn ensure_transfer_profiles_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.transfer_profiles_loaded {
                return;
            }
        }
        let data = match self
            .load_json_file::<Vec<TransferProfile>>("transfer_profiles.json")
            .await
        {
            Ok(profiles) => profiles
                .into_iter()
                .filter(|profile| profile.validate().is_ok())
                .collect(),
            Err(_) => Vec::new(),
        };
        let mut cache = self.cache.write().await;
        if cache.transfer_profiles_loaded {
            return;
        }
        cache.transfer_profiles = data;
        cache.transfer_profiles_loaded = true;
    }

    pub async fn get_transfer_profiles(&self) -> Vec<TransferProfile> {
        self.ensure_transfer_profiles_loaded().await;
        self.cache.read().await.transfer_profiles.clone()
    }

    pub async fn save_transfer_profile(
        &self,
        profile: TransferProfile,
    ) -> Result<(), DesktopRepositoryError> {
        profile
            .validate()
            .map_err(DesktopRepositoryError::ParseError)?;
        self.ensure_transfer_profiles_loaded().await;
        {
            let mut cache = self.cache.write().await;
            if let Some(existing) = cache
                .transfer_profiles
                .iter_mut()
                .find(|p| p.id == profile.id)
            {
                *existing = profile;
            } else {
                cache.transfer_profiles.push(profile);
            }
        }
        let snapshot = self.cache.read().await.transfer_profiles.clone();
        self.save_json_file("transfer_profiles.json", &snapshot)
            .await
    }

    pub async fn delete_transfer_profile(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        self.ensure_transfer_profiles_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.transfer_profiles.retain(|profile| profile.id != id);
        }
        let snapshot = self.cache.read().await.transfer_profiles.clone();
        self.save_json_file("transfer_profiles.json", &snapshot)
            .await
    }
}
