use super::models::SyncTask;
use super::{Store, StoreError};

impl Store {
    async fn ensure_sync_tasks_loaded(&self) {
        {
            let cache = self.cache.read().await;
            if cache.sync_tasks_loaded {
                return;
            }
        }
        let data = self
            .load_json_file::<Vec<SyncTask>>("sync_tasks.json")
            .await
            .unwrap_or_default();
        let mut cache = self.cache.write().await;
        if cache.sync_tasks_loaded {
            return;
        }
        cache.sync_tasks = data;
        cache.sync_tasks_loaded = true;
        tracing::debug!(
            count = cache.sync_tasks.len(),
            "Loaded sync tasks on demand"
        );
    }

    pub async fn get_sync_tasks(&self) -> Vec<SyncTask> {
        self.ensure_sync_tasks_loaded().await;
        let cache = self.cache.read().await;
        cache.sync_tasks.clone()
    }

    pub async fn save_sync_task(&self, task: SyncTask) -> Result<(), StoreError> {
        self.ensure_sync_tasks_loaded().await;
        {
            let mut cache = self.cache.write().await;
            if let Some(pos) = cache.sync_tasks.iter().position(|t| t.id == task.id) {
                cache.sync_tasks[pos] = task;
            } else {
                cache.sync_tasks.push(task);
            }
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.sync_tasks.clone()
        };
        self.save_json_file("sync_tasks.json", &snapshot).await
    }

    pub async fn delete_sync_task(&self, id: &str) -> Result<(), StoreError> {
        self.ensure_sync_tasks_loaded().await;
        {
            let mut cache = self.cache.write().await;
            cache.sync_tasks.retain(|t| t.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.sync_tasks.clone()
        };
        self.save_json_file("sync_tasks.json", &snapshot).await
    }
}
