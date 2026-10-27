//! The desktop store's side of the named views (08 §3.7).
//!
//! The views live in `{appData}/named_views.json` — the same family as
//! `settings.json` and `sync_profiles.json`: plain JSON, written atomically
//! through the store's own writer. There is no new storage engine and no new
//! encryption format.
//!
//! What lives here is **persistence only**. The rules — the name uniqueness
//! inside a scope, the one-default-per-scope rule, the ordering — live in the
//! shared layer (`datazen::named_views::domain`), because a
//! browser build has to reach the same answers.

use datazen::named_views::domain::{
    NamedView, NamedViewError, NamedViewIdMinter, NamedViewRepository,
};

use super::{DesktopRepository, DesktopRepositoryError};

/// The file the views live in.
const NAMED_VIEWS_FILE: &str = "named_views.json";

impl DesktopRepository {
    /// Load the views once, then keep them in the cache.
    ///
    /// Every record is parsed on its own: a record written by a newer build,
    /// or one that was damaged, must not hide the views around it.
    async fn ensure_named_views_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.named_views_loaded {
                return;
            }
        }
        let data = match self
            .load_json_file::<Vec<serde_json::Value>>(NAMED_VIEWS_FILE)
            .await
        {
            Ok(records) => records
                .into_iter()
                .filter_map(|record| serde_json::from_value::<NamedView>(record).ok())
                .filter(|view| view.validate().is_ok())
                .collect(),
            Err(_) => Vec::new(),
        };
        let mut cache = self.cache.write().await;
        if cache.named_views_loaded {
            return;
        }
        cache.named_views = data;
        cache.named_views_loaded = true;
    }

    /// Every stored view, across every scope.
    pub async fn get_named_views(&self) -> Vec<NamedView> {
        self.ensure_named_views_loaded().await;
        self.cache.read().await.named_views.clone()
    }

    /// Replace the stored collection.
    ///
    /// The whole file is written in one go, so a crash leaves either the old
    /// collection or the new one — never half of each.
    pub async fn set_named_views(&self, views: &[NamedView]) -> Result<(), DesktopRepositoryError> {
        self.ensure_named_views_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.named_views = views.to_vec();
        }
        let snapshot = self.cache.read().await.named_views.clone();
        self.save_json_file(NAMED_VIEWS_FILE, &snapshot).await
    }
}

/// The store is the desktop's implementation of the shared storage port.
#[async_trait::async_trait]
impl NamedViewRepository for DesktopRepository {
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        Ok(self.get_named_views().await)
    }

    async fn save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError> {
        self.set_named_views(views)
            .await
            .map_err(|error| NamedViewError::Storage(error.to_string()))
    }
}

/// The host's id minter: a ULID.
///
/// A ULID sorts by creation time, so the ids of two views saved in the same
/// second still tell which came first — the same reason the favorites use it.
pub struct HostViewIdMinter;

impl NamedViewIdMinter for HostViewIdMinter {
    fn mint_id(&self) -> String {
        super::favorites::ulid::new_ulid().to_string()
    }
}
