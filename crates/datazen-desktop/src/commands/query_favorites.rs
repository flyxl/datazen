//! Favorites IPC surface.
//!
//! Split out of `query.rs` because favorites are not query execution: since the
//! §2.6 change each one is a file in the user's own directory, and the panel
//! needs the root path to tell them so (plan §2.6.3).
//!
//! See `store::favorites` for the storage format and the one-time export from
//! the retired `favorite_queries` table.

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::store::{FavoriteQuery, NewFavorite};
use tauri::State;

#[tauri::command]
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Vec<FavoriteQuery>, CommandError> {
    state
        .store
        .get_favorite_queries(connection_id)
        .await
        .cmd_err("get_favorite_queries")
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    connection_id: String,
    title: String,
    sql: String,
) -> Result<FavoriteQuery, CommandError> {
    state
        .store
        .add_favorite_query(NewFavorite {
            connection_id,
            title,
            sql,
            database: None,
            keyword: None,
        })
        .await
        .cmd_err("add_favorite_query")
}

#[tauri::command]
pub async fn delete_favorite_query(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_favorite_query(&id)
        .await
        .cmd_err("delete_favorite_query")
}

/// Re-scan the favorites directory, picking up files a sync service delivered.
#[tauri::command]
pub async fn refresh_favorites(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Vec<FavoriteQuery>, CommandError> {
    state
        .store
        .refresh_favorites(connection_id)
        .await
        .cmd_err("refresh_favorites")
}

/// Directory the favorites are read from, so the panel can tell the user which
/// folder to point at a sync service (plan §2.6.3).
#[tauri::command]
pub async fn get_favorites_root(state: State<'_, AppState>) -> Result<String, CommandError> {
    Ok(state.store.favorites_root().display().to_string())
}

#[cfg(test)]
mod tests {
    use super::command_test_host::host_command_app;
    use super::*;
    use crate::store::favorites::ulid::Ulid;
    use crate::testing::app_state_test_support::TestAppState;

    #[tokio::test]
    async fn favorite_queries_roundtrip() {
        let app = host_command_app(TestAppState::new().await);
        assert!(get_favorite_queries(app.state(), None)
            .await
            .unwrap()
            .is_empty());

        let fav = add_favorite_query(
            app.state(),
            "cfg-test".into(),
            "My query".into(),
            "SELECT 1".into(),
        )
        .await
        .unwrap();

        // The favorite is a file, not a row: the id is the file name.
        assert!(Ulid::is_valid(&fav.id), "id {} should be a ULID", fav.id);
        let root = get_favorites_root(app.state()).await.unwrap();
        let file = std::path::Path::new(&root).join(format!("{}.sql", fav.id));
        assert!(file.is_file(), "expected {} to exist", file.display());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            format!(
                "-- title: My query\n\
                 -- connectionId: cfg-test\n\
                 -- createdAt: {}\n\
                 -- updatedAt: {}\n\
                 SELECT 1\n",
                fav.created_at.to_rfc3339(),
                fav.updated_at
                    .expect("a new favorite is dated")
                    .to_rfc3339(),
            )
        );

        assert_eq!(
            get_favorite_queries(app.state(), Some("cfg-test".into()))
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(get_favorite_queries(app.state(), Some("other".into()))
            .await
            .unwrap()
            .is_empty());

        delete_favorite_query(app.state(), fav.id.clone())
            .await
            .unwrap();
        assert!(get_favorite_queries(app.state(), None)
            .await
            .unwrap()
            .is_empty());
        assert!(!file.exists(), "deleted favorite leaves the live tree");

        // Soft delete: the SQL is still on disk under .trash, under a
        // timestamped name that still carries the id.
        let trash_dir = std::path::Path::new(&root).join(".trash");
        let trashed: Vec<std::path::PathBuf> = std::fs::read_dir(&trash_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(trashed.len(), 1, "got {trashed:?}");
        assert!(
            std::fs::read_to_string(&trashed[0])
                .unwrap()
                .ends_with("SELECT 1\n"),
            "the statement is still runnable after a delete"
        );
        assert!(
            trashed[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(&format!("__{}.sql", fav.id)),
            "got: {}",
            trashed[0].display()
        );
    }

    #[tokio::test]
    async fn a_favorite_for_another_connection_is_never_listed() {
        // The filter is the whole point of storing `connectionId` in the
        // front-matter: switching connections must not show another database's
        // saved SQL.
        let app = host_command_app(TestAppState::new().await);
        add_favorite_query(app.state(), "cfg-a".into(), "A".into(), "SELECT 1".into())
            .await
            .unwrap();
        add_favorite_query(app.state(), "cfg-b".into(), "B".into(), "SELECT 2".into())
            .await
            .unwrap();

        assert_eq!(
            get_favorite_queries(app.state(), Some("cfg-a".into()))
                .await
                .unwrap()[0]
                .title,
            "A"
        );
        assert_eq!(
            get_favorite_queries(app.state(), Some("cfg-b".into()))
                .await
                .unwrap()[0]
                .title,
            "B"
        );
        assert!(get_favorite_queries(app.state(), Some("cfg-c".into()))
            .await
            .unwrap()
            .is_empty());
    }
}

// 测试宿主（cfg(test)）：B2 批次 4 刀移除 `*_impl` 后，收藏测试经真实
// `#[tauri::command]` 入口驱动（与 data/query 各域宿主同形）。声明置于
// `mod tests` 之后：生产源以首个 `#[cfg(test)]` 分界（config.rs 首立约定）。
#[cfg(test)]
#[path = "query_favorites_command_test_host.rs"]
mod command_test_host;
