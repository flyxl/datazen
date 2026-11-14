//! A JSON-file [`ConnectionRepository`] for the native host.
//!
//! The store is one object holding every connection config in user order
//! plus the custom group list. Writes are atomic — a sibling temporary file
//! is renamed over the store — so a crash mid-write leaves the previous
//! file whole. A missing file is a fresh install (no connections); a file
//! that exists but does not parse refuses to open, so user data is never
//! silently replaced by an empty store.

use async_trait::async_trait;
use datazen::connection::domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope,
};
use datazen_driver_api::ConnectionConfig;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The file's shape. Field names follow `ConnectionConfig`'s camelCase.
#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
struct StoreFile {
    #[serde(default)]
    connections: Vec<ConnectionConfig>,
    #[serde(default)]
    groups: Vec<String>,
}

/// The single-tenant scope every method of this host serves; a scope with
/// another organization sees nothing, as the boundary semantics say.
#[cfg(test)]
fn desktop_scope() -> ConnectionScope {
    ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION)
}

pub struct FileConnectionRepository {
    path: PathBuf,
    /// The loaded store. One lock guards the file and the loaded list, so
    /// the two cannot part ways between a read and its write-back.
    state: Mutex<StoreFile>,
}

impl std::fmt::Debug for FileConnectionRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileConnectionRepository").field("path", &self.path).finish()
    }
}

impl FileConnectionRepository {
    /// Open the store at `path`. A missing file opens empty — a fresh
    /// install has no connections, which is not an error. A file that
    /// exists but cannot be read or parsed is an error: opening it would
    /// have the next save replace the user's data with an empty store.
    pub fn open(path: &Path) -> Result<Self, String> {
        let state = match std::fs::read_to_string(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreFile::default(),
            Err(error) => return Err(format!("read {}: {error}", path.display())),
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("parse {}: {error}", path.display()))?,
        };
        Ok(Self { path: path.to_owned(), state: Mutex::new(state) })
    }

    /// Write the store atomically: a sibling temporary file, then rename.
    fn persist(path: &Path, store: &StoreFile) -> Result<(), ConnectionRepositoryError> {
        let text = serde_json::to_string_pretty(store).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!("serialize the store: {error}"))
        })?;
        let temp = path.with_extension("json.tmp");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        std::fs::write(&temp, text).map_err(io_error)?;
        std::fs::rename(&temp, path).map_err(io_error)?;
        Ok(())
    }
}

fn io_error(error: std::io::Error) -> ConnectionRepositoryError {
    ConnectionRepositoryError::BackendUnavailable(error.to_string())
}

fn scope_error() -> ConnectionRepositoryError {
    ConnectionRepositoryError::BackendUnavailable("out of the desktop scope".into())
}

#[async_trait]
impl ConnectionRepository for FileConnectionRepository {
    async fn list(&self, scope: &ConnectionScope) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        Ok(self.state.lock().expect("the store lock").connections.clone())
    }

    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        Ok(self
            .state
            .lock()
            .expect("the store lock")
            .connections
            .iter()
            .find(|config| config.id == connection_id)
            .cloned())
    }

    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let mut state = self.state.lock().expect("the store lock");
        match state.connections.iter_mut().find(|saved| saved.id == config.id) {
            Some(saved) => *saved = config,
            None => state.connections.push(config),
        }
        Self::persist(&self.path, &state)
    }

    async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let mut state = self.state.lock().expect("the store lock");
        let before = state.connections.len();
        state.connections.retain(|config| config.id != connection_id);
        if state.connections.len() == before {
            return Ok(()); // an absent id is already gone
        }
        Self::persist(&self.path, &state)
    }

    async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let mut state = self.state.lock().expect("the store lock");
        let mut ordered = Vec::with_capacity(state.connections.len());
        for id in &ordered_ids {
            if let Some(index) = state.connections.iter().position(|config| &config.id == id) {
                ordered.push(state.connections.remove(index));
            }
        }
        ordered.append(&mut state.connections);
        state.connections = ordered;
        Self::persist(&self.path, &state)
    }

    async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let mut state = self.state.lock().expect("the store lock");
        let stamp = chrono::Utc::now().to_rfc3339();
        match state.connections.iter_mut().find(|config| config.id == connection_id) {
            Some(config) => config.last_connected_at = Some(stamp),
            None => return Ok(()), // nothing to stamp
        }
        Self::persist(&self.path, &state)
    }

    async fn groups(&self, scope: &ConnectionScope) -> Result<Vec<String>, ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let state = self.state.lock().expect("the store lock");
        let mut groups = state.groups.clone();
        for config in &state.connections {
            if let Some(group) = &config.group {
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
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let mut state = self.state.lock().expect("the store lock");
        state.groups = groups;
        Self::persist(&self.path, &state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen::connection::application::ConnectionService;
    use datazen_ui_api::DesktopAdapter;
    use std::sync::Arc;

    /// A config with the fields the round trips exercise; the rest at their
    /// defaults, spelled out the way the adapter's `config_from_profile`
    /// fills them (the struct has no `Default`).
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
        let dir = std::env::temp_dir().join(format!("datazen-rune-host-{tag}-{}", std::process::id()));
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
        assert!(raw.contains("\"warehouse\""), "the id is in the file: {raw}");
        let reopened = FileConnectionRepository::open(&path).unwrap();
        let listed =
            futures_executor::block_on(service(reopened).list_redacted(&desktop_scope())).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "warehouse");
        assert_eq!(listed[0].name, "Warehouse");
        assert_ne!(listed[0].password.as_deref(), Some("secret"), "the list is redacted");
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

        let service = ConnectionService::new(Arc::new(
            FileConnectionRepository::open(&path).unwrap(),
        ));
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
    fn a_corrupt_file_refuses_to_open() {
        let (path, _store) = temp_store("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        let error = FileConnectionRepository::open(&path).unwrap_err();
        assert!(error.contains("parse"), "the sentence names the failure: {error}");
    }

    #[test]
    fn the_adapter_lists_the_file_store_through_the_desktop_seam() {
        // The seam N2 exists for: the adapter the live host injects lists
        // what the file holds, not the demo seeds.
        let (path, store) = temp_store("seam");
        let service = service(store);
        futures_executor::block_on(service.save(&desktop_scope(), config("warehouse", "Warehouse")))
            .unwrap();

        let repository = Arc::new(FileConnectionRepository::open(&path).unwrap());
        let mut adapter = datazen_ui_api::UseCaseAdapter::from_repository(repository);
        let listed = adapter.list_connections().unwrap();
        assert_eq!(listed.len(), 1, "the file's connection, not the demo list");
        assert_eq!(listed[0].id, "warehouse");
        assert_eq!(listed[0].name, "Warehouse");
        assert_eq!(listed[0].engine, "sqlite");
    }
}