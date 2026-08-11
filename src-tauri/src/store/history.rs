use super::models::{FavoriteQuery, QueryHistoryEntry};
use super::{Store, StoreError};

impl Store {
    async fn ensure_favorite_queries_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.favorite_queries_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<FavoriteQuery>>("favorites/queries.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.favorite_queries_loaded {
            return;
        }
        cache.favorite_queries = data;
        cache.favorite_queries_loaded = true;
        tracing::debug!(
            count = cache.favorite_queries.len(),
            "Loaded favorite queries on demand"
        );
    }

    pub async fn add_query_history(&self, entry: QueryHistoryEntry) -> Result<(), StoreError> {
        self.history_db
            .add_query_history(entry)
            .map_err(|e| StoreError::WriteError(e.to_string()))
    }

    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        self.history_db
            .get_query_history(limit)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to read query history from SQLite");
                Vec::new()
            })
    }

    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        self.purge_history(super::HistoryScope::Query, None).await?;
        Ok(())
    }

    pub async fn purge_history(
        &self,
        scope: super::HistoryScope,
        retain_days: Option<u32>,
    ) -> Result<u64, StoreError> {
        self.history_db
            .purge(scope, retain_days)
            .map_err(|e| StoreError::WriteError(e.to_string()))
    }

    pub async fn get_favorite_queries(&self) -> Vec<FavoriteQuery> {
        self.ensure_favorite_queries_loaded().await;
        let cache = self.cache.read().await;
        cache.favorite_queries.clone()
    }

    pub async fn add_favorite_query(&self, fav: FavoriteQuery) -> Result<(), StoreError> {
        self.ensure_favorite_queries_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.favorite_queries.insert(0, fav);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.favorite_queries.clone()
        };
        self.save_json_file("favorites/queries.json", &snapshot).await
    }

    pub async fn delete_favorite_query(&self, id: &str) -> Result<(), StoreError> {
        self.ensure_favorite_queries_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.favorite_queries.retain(|f| f.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.favorite_queries.clone()
        };
        self.save_json_file("favorites/queries.json", &snapshot).await
    }
}
