//! A JSON-file [`ConnectionRepository`] for the native host.
//!
//! The store is one object holding every connection config in user order
//! plus the custom group list. Writes are atomic — a sibling temporary file
//! is renamed over the store — so a crash mid-write leaves the previous
//! file whole. A missing file is a fresh install (no connections); a file
//! that exists but does not parse refuses to open, so user data is never
//! silently replaced by an empty store.

#[cfg(test)]
mod credential_tests;
mod credentials;
mod disk;
mod record;
use record::StoredConnection;

use async_trait::async_trait;
use datazen::connection::domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope,
};
use datazen_driver_api::ConnectionConfig;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The file's shape. Field names follow `ConnectionConfig`'s camelCase.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct StoreFile {
    #[serde(default)]
    connections: Vec<StoredConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    groups: Option<Vec<String>>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
use crate::host_connection_scope as desktop_scope;

pub struct FileConnectionRepository {
    path: PathBuf,
    /// T10: the password encryption key, loaded on the first password that
    /// needs one. A store without passwords never touches the keychain.
    key: Mutex<Option<[u8; 32]>>,
    load_key: fn(&Path, bool) -> Result<[u8; 32], String>,
}

impl std::fmt::Debug for FileConnectionRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileConnectionRepository")
            .finish_non_exhaustive()
    }
}

impl FileConnectionRepository {
    /// Open the store at `path`. A missing file opens empty — a fresh
    /// install has no connections, which is not an error. A file that
    /// exists but cannot be read or parsed is an error: opening it would
    /// have the next save replace the user's data with an empty store.
    pub fn open(path: &Path) -> Result<Self, String> {
        StoreFile::read(path).map_err(|error| error.to_string())?;
        Ok(Self {
            path: path.to_owned(),
            key: Mutex::new(None),
            load_key: |dir, allow_create| {
                crate::secrets::load_master_key(
                    dir,
                    crate::secrets::resolve_key_backend(),
                    allow_create,
                )
            },
        })
    }

    fn update(
        &self,
        change: impl FnOnce(&mut StoreFile) -> Result<bool, ConnectionRepositoryError>,
    ) -> Result<(), ConnectionRepositoryError> {
        self.transaction(|store| Ok(((), change(store)?)))
    }
}

fn check_scope(scope: &ConnectionScope) -> Result<(), ConnectionRepositoryError> {
    if scope.organization_id() == crate::host_request_context().organization_id.as_str() {
        Ok(())
    } else {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "out of the desktop scope".into(),
        ))
    }
}

#[async_trait]
impl ConnectionRepository for FileConnectionRepository {
    async fn list(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        check_scope(scope)?;
        self.read_connections(None)
    }

    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        check_scope(scope)?;
        Ok(self
            .read_connections(Some(connection_id))?
            .into_iter()
            .next())
    }

    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        check_scope(scope)?;
        self.save_config(config)
    }

    async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        check_scope(scope)?;
        self.update(|state| {
            let before = state.connections.len();
            state
                .connections
                .retain(|row| row.config().id != connection_id);
            Ok(state.connections.len() != before)
        })
    }

    async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        check_scope(scope)?;
        self.update(|state| {
            let mut ordered = Vec::with_capacity(state.connections.len());
            for id in &ordered_ids {
                if let Some(index) = state
                    .connections
                    .iter()
                    .position(|row| &row.config().id == id)
                {
                    ordered.push(state.connections.remove(index));
                }
            }
            ordered.append(&mut state.connections);
            state.connections = ordered;
            Ok(true)
        })
    }

    async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        check_scope(scope)?;
        self.update(|state| {
            let Some(row) = state
                .connections
                .iter_mut()
                .find(|row| row.config().id == connection_id)
            else {
                return Ok(false);
            };
            row.set_last_connected_at(chrono::Utc::now().to_rfc3339());
            Ok(true)
        })
    }

    async fn groups(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        check_scope(scope)?;
        let state = StoreFile::read(&self.path)?;
        let mut groups = state.groups.unwrap_or_default();
        for row in &state.connections {
            if let Some(group) = &row.config().group {
                if !groups.contains(group) {
                    groups.push(group.clone());
                }
            }
        }
        Ok(groups)
    }

    async fn save_groups(
        &self,
        scope: &ConnectionScope,
        groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        check_scope(scope)?;
        self.update(|state| {
            state.groups = Some(groups);
            Ok(true)
        })
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Fixtures shared by the host crate's test modules.

    use datazen_driver_api::{ConnectionConfig, SslMode};

    /// A sqlite config pointing at `database` (the file path). Every field
    /// spelled out — the struct has no `Default` — the way the adapter's
    /// `config_from_profile` fills them.
    pub(crate) fn sqlite_config(id: &str, database: Option<String>) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: "sqlite".into(),
            host: None,
            port: None,
            database,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::connection::application::ConnectionService;
    use datazen_ui_api::DesktopAdapter;
    use std::sync::Arc;

    /// A stored profile with credentials for round-trip and redaction tests.
    fn config(id: &str, name: &str) -> ConnectionConfig {
        use datazen_driver_api::SslMode;
        ConnectionConfig {
            id: id.into(),
            name: name.into(),
            database_type: "sqlite".into(),
            host: None,
            port: None,
            database: Some(format!("{id}.db")),
            schema: None,
            username: Some("user".into()),
            password: Some("secret".into()),
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    fn service(repository: FileConnectionRepository) -> ConnectionService {
        ConnectionService::new(Arc::new(repository))
    }

    fn temp_store(tag: &str) -> (PathBuf, FileConnectionRepository) {
        // The store tests opt into the file key backend: the OS keychain is
        // unreachable from a test environment without a session bus (the
        // desktop's own CI discipline, ).
        std::env::set_var("DATAZEN_NATIVE_KEYRING", "file");
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-host-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("connections.json");
        let store = FileConnectionRepository::open(&path).unwrap();
        (path, store)
    }

    #[test]
    fn a_missing_file_opens_empty_and_the_use_case_lists_nothing() {
        let (path, store) = temp_store("missing");
        assert!(!path.exists(), "nothing is created until the first save");
        let listed =
            futures_executor::block_on(service(store).list_redacted(&desktop_scope())).unwrap();
        assert!(listed.is_empty(), "a fresh install lists no connections");
    }

    #[test]
    fn a_saved_connection_round_trips_through_the_use_case_and_the_file() {
        let (path, store) = temp_store("round-trip");
        futures_executor::block_on(
            service(store).save(&desktop_scope(), config("warehouse", "Warehouse")),
        )
        .unwrap();

        // The file holds the config, and a fresh process over the same file
        // sees it: the store's whole point.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"warehouse\""),
            "the id is in the file: {raw}"
        );
        let reopened = FileConnectionRepository::open(&path).unwrap();
        let listed =
            futures_executor::block_on(service(reopened).list_redacted(&desktop_scope())).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "warehouse");
        assert_eq!(listed[0].name, "Warehouse");
        assert_ne!(
            listed[0].password.as_deref(),
            Some("secret"),
            "the list is redacted"
        );
    }

    #[test]
    fn delete_and_reorder_land_in_the_file() {
        let (path, store) = temp_store("mutations");
        let service = service(store);
        let scope = desktop_scope();
        for id in ["alpha", "beta", "gamma"] {
            futures_executor::block_on(service.save(&scope, config(id, id))).unwrap();
        }
        futures_executor::block_on(service.reorder(&scope, vec!["gamma".into(), "alpha".into()]))
            .unwrap();
        futures_executor::block_on(service.delete(&scope, "beta")).unwrap();

        let service =
            ConnectionService::new(Arc::new(FileConnectionRepository::open(&path).unwrap()));
        let listed = futures_executor::block_on(service.list_plain(&scope)).unwrap();
        let ids: Vec<&str> = listed.iter().map(|config| config.id.as_str()).collect();
        assert_eq!(ids, vec!["gamma", "alpha"], "reorder kept, delete removed");

        futures_executor::block_on(service.touch_last_connected_at(&scope, "gamma")).unwrap();
        let listed = futures_executor::block_on(service.list_plain(&scope)).unwrap();
        assert!(
            listed[0].last_connected_at.is_some(),
            "the connect stamp landed"
        );
    }

    #[test]
    fn a_failed_delete_can_be_retried_without_losing_the_saved_connection() {
        let (path, store) = temp_store("delete-retry");
        let scope = desktop_scope();
        let config = test_support::sqlite_config("local", None);
        futures_executor::block_on(async {
            store.save(&scope, config).await.unwrap();
            let obstruction = path.with_added_extension("lock");
            std::fs::remove_file(&obstruction).unwrap();
            std::fs::create_dir(&obstruction).unwrap();
            assert!(store.delete(&scope, "local").await.is_err());
            assert!(store.get(&scope, "local").await.unwrap().is_some());

            std::fs::remove_dir(obstruction).unwrap();
            store.delete(&scope, "local").await.unwrap();
            let reopened = FileConnectionRepository::open(&path).unwrap();
            assert!(reopened.get(&scope, "local").await.unwrap().is_none());
        });
    }

    #[test]
    fn a_corrupt_file_refuses_to_open() {
        let (path, _store) = temp_store("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let error = FileConnectionRepository::open(&path).unwrap_err();
        assert!(
            error.contains("parse"),
            "the sentence names the failure: {error}"
        );
    }

    #[test]
    fn the_adapter_lists_the_file_store_through_the_desktop_seam() {
        let (path, store) = temp_store("seam");
        let service = service(store);
        let scope = desktop_scope();
        futures_executor::block_on(async {
            service
                .save(&scope, config("warehouse", "Warehouse"))
                .await
                .unwrap();
            service
                .save(&scope, config("reports", "Reports"))
                .await
                .unwrap();
            service
                .reorder(&scope, vec!["reports".into(), "warehouse".into()])
                .await
                .unwrap();
        });

        let repository = Arc::new(FileConnectionRepository::open(&path).unwrap());
        let mut adapter = crate::LiveAdapter::with_data_dir(repository, path.parent().unwrap());
        let listed = adapter.list_connections().unwrap();
        let profiles: Vec<_> = listed
            .iter()
            .map(|profile| {
                (
                    profile.id.as_str(),
                    profile.name.as_str(),
                    profile.engine.as_str(),
                )
            })
            .collect();
        assert_eq!(
            profiles,
            [
                ("reports", "Reports", "sqlite"),
                ("warehouse", "Warehouse", "sqlite")
            ]
        );
    }

    #[test]
    fn passwords_seal_on_save_and_reveal_on_get() {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-store-seal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("connections.json");

        std::env::set_var("DATAZEN_NATIVE_KEYRING", "file");
        let scope = desktop_scope();
        let store = FileConnectionRepository::open(&path).unwrap();
        let mut config = test_support::sqlite_config("secret", None);
        config.password = Some("hunter2".into());
        futures_executor::block_on(store.save(&scope, config)).unwrap();

        // The file on disk carries no plaintext password.
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(
            !on_disk.contains("hunter2"),
            "the plaintext never lands: {on_disk}"
        );
        assert!(
            on_disk.contains("password"),
            "the field is there, sealed: {on_disk}"
        );

        // A fresh store over the same file reveals it on get.
        let store = FileConnectionRepository::open(&path).unwrap();
        let got = futures_executor::block_on(store.get(&scope, "secret"))
            .unwrap()
            .expect("the connection is there");
        assert_eq!(got.password.as_deref(), Some("hunter2"));

        // The listing reveals it too (best-effort path, key available).
        let listed = futures_executor::block_on(store.list(&scope)).unwrap();
        assert_eq!(
            listed
                .iter()
                .find(|c| c.id == "secret")
                .and_then(|c| c.password.clone()),
            Some("hunter2".into())
        );
        std::env::remove_var("DATAZEN_NATIVE_KEYRING");
    }

    #[test]
    fn an_encrypted_password_without_a_key_is_an_honest_unavailability() {
        // Write a sealed password with the file backend in one directory,
        // then move only the store to another with an unavailable keyring.
        // The reveal must report a missing key and keep the ciphertext private.
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-store-closed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let write_dir = dir.join("write");
        std::fs::create_dir_all(&write_dir).unwrap();
        let scope = desktop_scope();
        let mut store =
            FileConnectionRepository::open(&write_dir.join("connections.json")).unwrap();
        store.load_key = |dir, allow_create| {
            crate::secrets::load_master_key(dir, crate::secrets::KeyBackend::File, allow_create)
        };
        let mut config = test_support::sqlite_config("sealed", None);
        config.password = Some("open-sesame".into());
        futures_executor::block_on(store.save(&scope, config)).unwrap();

        // Only the store moves; the key file stays behind.
        let read_dir = dir.join("read");
        std::fs::create_dir_all(&read_dir).unwrap();
        std::fs::copy(
            write_dir.join("connections.json"),
            read_dir.join("connections.json"),
        )
        .unwrap();

        let mut store = FileConnectionRepository::open(&read_dir.join("connections.json")).unwrap();
        store.load_key = crate::secrets::unavailable_keyring;
        // The keyring backend cannot be reached and no key file exists: the
        // reveal fails closed.
        let error = futures_executor::block_on(store.get(&scope, "sealed"))
            .err()
            .expect("missing key must be reported");
        assert!(
            error
                .to_string()
                .contains("saved connection password is unavailable"),
            "the sentence names the unavailability: {error}"
        );
        assert!(
            error.to_string().contains("DATAZEN_NATIVE_KEYRING=file"),
            "the sentence names the escape hatch: {error}"
        );
        // The listing stays whole: the password is absent, not leaked.
        let listed = futures_executor::block_on(store.list(&scope)).unwrap();
        let listed = listed.iter().find(|c| c.id == "sealed").unwrap();
        assert!(listed.password.is_none(), "no ciphertext in the listing");
    }
}
