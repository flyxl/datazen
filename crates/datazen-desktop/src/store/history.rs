//! DesktopRepository 的 favorites 门面。
//!
//! D6 第一切片（2026-10-26）后本文件不再承载历史门面：查询历史与迁移运行
//! 历史的词汇、仓储端口与用例归 `datazen::history`；桌面 SQLite 实现经
//! `platform::history::DesktopHistoryRepositories` 适配，所有消费者（命令、
//! MCP 资源、AI 分析、会话执行路径）走 `require_history_service` / 注入的
//! `QueryHistoryRepository` 端口。`HistoryDb` 本体（SQL、schema 迁移环、保留
//! 截断）仍在 `store::history_db`。

use std::sync::Arc;

use super::favorites::NewFavorite;
use super::models::FavoriteQuery;
use super::{DesktopRepository, DesktopRepositoryError};

impl DesktopRepository {
    // ── Favorites (file-backed, see `store::favorites`) ───────────────────

    /// Directory the favorites are read from. Surfaced in the UI so the user
    /// knows which folder to point at a sync service (plan §2.6.3).
    pub fn favorites_root(&self) -> std::path::PathBuf {
        self.favorites.root()
    }

    /// Repoint the favorites at a new directory (a settings change).
    pub async fn set_favorites_root(&self, root: &str) -> Result<(), DesktopRepositoryError> {
        let root = root.trim();
        if root.is_empty() {
            return Err(DesktopRepositoryError::InitError(
                "Favorites directory must not be empty".into(),
            ));
        }
        self.favorites
            .set_root(std::path::Path::new(root))
            .map_err(|e| DesktopRepositoryError::InitError(e.to_string()))
    }

    /// List favorites for one connection (`None` = every connection).
    ///
    /// Disk I/O runs on the blocking pool: a cold start reads every `.sql` file
    /// under the root, and doing that on a runtime worker would stall command
    /// dispatch for the duration.
    pub async fn get_favorite_queries(
        &self,
        connection_id: Option<String>,
    ) -> Result<Vec<FavoriteQuery>, DesktopRepositoryError> {
        let favorites = Arc::clone(&self.favorites);
        let filter = connection_id.clone();
        tokio::task::spawn_blocking(move || favorites.list(filter.as_deref()))
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(format!("Favorites task failed: {e}")))
    }

    /// Save a new favorite as its own `.sql` file.
    pub async fn add_favorite_query(
        &self,
        draft: NewFavorite,
    ) -> Result<FavoriteQuery, DesktopRepositoryError> {
        let favorites = Arc::clone(&self.favorites);
        tokio::task::spawn_blocking(move || favorites.add(draft))
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(format!("Favorites task failed: {e}")))?
            .map_err(|e| DesktopRepositoryError::WriteError(e.to_string()))
    }

    /// Move a favorite into `.trash/`.
    pub async fn delete_favorite_query(&self, id: &str) -> Result<(), DesktopRepositoryError> {
        let favorites = Arc::clone(&self.favorites);
        let id = id.to_string();
        tokio::task::spawn_blocking(move || favorites.delete(&id))
            .await
            .map_err(|e| DesktopRepositoryError::ReadError(format!("Favorites task failed: {e}")))?
            .map_err(|e| DesktopRepositoryError::WriteError(e.to_string()))
    }

    /// Re-read the favorites directory and return the fresh listing.
    ///
    /// The in-memory cache is only invalidated by app-side writes, so a folder
    /// delivered by iCloud/Dropbox/git stays invisible until this runs. The UI
    /// calls it when the panel is shown and when the window regains focus —
    /// those are exactly the moments a file can have arrived behind our back.
    pub async fn refresh_favorites(
        &self,
        connection_id: Option<String>,
    ) -> Result<Vec<FavoriteQuery>, DesktopRepositoryError> {
        self.favorites.invalidate_cache();
        self.get_favorite_queries(connection_id).await
    }
}
