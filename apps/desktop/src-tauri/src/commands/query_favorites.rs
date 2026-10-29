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

pub(crate) async fn get_favorite_queries_impl(
    state: &AppState,
    connection_id: Option<String>,
) -> Result<Vec<FavoriteQuery>, CommandError> {
    state
        .store
        .get_favorite_queries(connection_id)
        .await
        .cmd_err("get_favorite_queries")
}

pub(crate) async fn add_favorite_query_impl(
    state: &AppState,
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

pub(crate) async fn delete_favorite_query_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_favorite_query(&id)
        .await
        .cmd_err("delete_favorite_query")
}

/// Re-scan the favorites directory, picking up files a sync service delivered.
pub(crate) async fn refresh_favorites_impl(
    state: &AppState,
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
pub(crate) async fn get_favorites_root_impl(state: &AppState) -> Result<String, CommandError> {
    Ok(state.store.favorites_root().display().to_string())
}

#[tauri::command]
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Vec<FavoriteQuery>, CommandError> {
    get_favorite_queries_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    connection_id: String,
    title: String,
    sql: String,
) -> Result<FavoriteQuery, CommandError> {
    add_favorite_query_impl(&state, connection_id, title, sql).await
}

#[tauri::command]
pub async fn delete_favorite_query(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    delete_favorite_query_impl(&state, id).await
}

#[tauri::command]
pub async fn get_favorites_root(state: State<'_, AppState>) -> Result<String, CommandError> {
    get_favorites_root_impl(&state).await
}

#[tauri::command]
pub async fn refresh_favorites(
    state: State<'_, AppState>,
    connection_id: Option<String>,
) -> Result<Vec<FavoriteQuery>, CommandError> {
    refresh_favorites_impl(&state, connection_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::favorites::ulid::Ulid;
    use crate::testing::app_state::TestAppState;

    #[tokio::test]
    async fn favorite_queries_roundtrip() {
        let test = TestAppState::new().await;
        assert!(get_favorite_queries_impl(&test.state, None)
            .await
            .unwrap()
            .is_empty());

        let fav = add_favorite_query_impl(
            &test.state,
            "cfg-test".into(),
            "My query".into(),
            "SELECT 1".into(),
        )
        .await
        .unwrap();

        // The favorite is a file, not a row: the id is the file name.
        assert!(Ulid::is_valid(&fav.id), "id {} should be a ULID", fav.id);
        let root = get_favorites_root_impl(&test.state).await.unwrap();
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
            get_favorite_queries_impl(&test.state, Some("cfg-test".into()))
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(get_favorite_queries_impl(&test.state, Some("other".into()))
            .await
            .unwrap()
            .is_empty());

        delete_favorite_query_impl(&test.state, fav.id.clone())
            .await
            .unwrap();
        assert!(get_favorite_queries_impl(&test.state, None)
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
        let test = TestAppState::new().await;
        add_favorite_query_impl(&test.state, "cfg-a".into(), "A".into(), "SELECT 1".into())
            .await
            .unwrap();
        add_favorite_query_impl(&test.state, "cfg-b".into(), "B".into(), "SELECT 2".into())
            .await
            .unwrap();

        assert_eq!(
            get_favorite_queries_impl(&test.state, Some("cfg-a".into()))
                .await
                .unwrap()[0]
                .title,
            "A"
        );
        assert_eq!(
            get_favorite_queries_impl(&test.state, Some("cfg-b".into()))
                .await
                .unwrap()[0]
                .title,
            "B"
        );
        assert!(get_favorite_queries_impl(&test.state, Some("cfg-c".into()))
            .await
            .unwrap()
            .is_empty());
    }
}
