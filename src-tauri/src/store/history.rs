use super::models::{FavoriteQuery, QueryHistoryEntry};
use super::{Store, StoreError};

impl Store {
    async fn ensure_query_history_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.query_history_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<QueryHistoryEntry>>("history/queries.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.query_history_loaded {
            return;
        }
        cache.query_history = data;
        cache.query_history_loaded = true;
        tracing::debug!(
            count = cache.query_history.len(),
            "Loaded query history on demand"
        );
    }

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
        self.ensure_query_history_loaded().await;
        {
            let mut cache = self.cache.write().await;
            let dominated = cache
                .query_history
                .first()
                .map(|last| last.sql.trim() == entry.sql.trim())
                .unwrap_or(false);
            if dominated {
                if let Some(first) = cache.query_history.first_mut() {
                    first.executed_at = entry.executed_at;
                    first.execution_time_ms = entry.execution_time_ms;
                    first.rows_affected = entry.rows_affected;
                    first.success = entry.success;
                    first.error_message = entry.error_message.clone();
                }
            } else {
                cache.query_history.insert(0, entry);
                if cache.query_history.len() > 1000 {
                    cache.query_history.truncate(1000);
                }
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.query_history.clone()
        };

        self.save_json_file("history/queries.json", &snapshot).await
    }

    pub async fn get_query_history(&self, limit: usize) -> Vec<QueryHistoryEntry> {
        self.ensure_query_history_loaded().await;
        let cache = self.cache.read().await;
        cache.query_history.iter().take(limit).cloned().collect()
    }

    pub async fn clear_query_history(&self) -> Result<(), StoreError> {
        self.ensure_query_history_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.query_history.clear();
            cache.query_history_loaded = true;
        }
        self.save_json_file("history/queries.json", &Vec::<QueryHistoryEntry>::new())
            .await
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
