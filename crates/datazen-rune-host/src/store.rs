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
    /// T10: the password encryption key, loaded on the first password that
    /// needs one. A store without passwords never touches the keychain.
    key: Mutex<Option<[u8; 32]>>,
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
        Ok(Self { path: path.to_owned(), state: Mutex::new(state), key: Mutex::new(None) })
    }

    /// Write the store atomically: a sibling temporary file, then rename.
    /// T10: the password encryption key, loaded on first use. The store's
    /// directory is the key file's home (the file backend) and the source
    /// of a legacy rescue key (the keyring backend).
    fn master_key(&self) -> Result<[u8; 32], ConnectionRepositoryError> {
        let mut cached = self.key.lock().expect("the key lock");
        if let Some(key) = *cached {
            return Ok(key);
        }
        let data_dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let loaded = crate::secrets::load_or_create_master_key(data_dir, crate::secrets::resolve_key_backend())
            .map_err(ConnectionRepositoryError::BackendUnavailable)?;
        *cached = Some(loaded);
        Ok(loaded)
    }

    /// Encrypt a password for the file (T10). An empty password is stored
    /// as-is — there is nothing to protect.
    fn seal_password(&self, password: &str) -> Result<String, ConnectionRepositoryError> {
        let key = self.master_key()?;
        datazen::connection_import::encrypt_field(password, &key)
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.to_string()))
    }

    /// Reveal a stored password (T10): a value that decrypts is the
    /// plaintext; one that does not is a legacy plaintext from before the
    /// key, kept working as the migration path. `None` key means the key
    /// could not be loaded — the caller decides what that means.
    fn open_password(&self, stored: &str) -> Option<String> {
        let key = self.master_key().ok()?;
        datazen::connection_import::decrypt_field(stored, &key).ok()
    }

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
        let mut connections = self.state.lock().expect("the store lock").connections.clone();
        // T10: passwords reveal best-effort here — the listing page shows
        // names, and the connect path's `get` carries the honest
        // unavailability sentence when a password cannot be opened. A
        // password the key cannot open becomes None in the listing, never a
        // leaked ciphertext.
        for config in &mut connections {
            if let Some(password) = config.password.take() {
                config.password = if password.is_empty() {
                    Some(password)
                } else if self.master_key().is_ok() {
                    // A decrypting value is the plaintext; one that does not
                    // is a legacy plaintext (the migration path).
                    Some(self.open_password(&password).unwrap_or(password))
                } else {
                    None
                };
            }
        }
        Ok(connections)
    }

    async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        let stored = self
            .state
            .lock()
            .expect("the store lock")
            .connections
            .iter()
            .find(|config| config.id == connection_id)
            .cloned();
        let Some(mut config) = stored else { return Ok(None) };
        if let Some(password) = config.password.take() {
            if password.is_empty() {
                config.password = Some(password);
            } else {
                match self.master_key() {
                    Ok(_) => {
                        // A decrypting value is the plaintext; one that does
                        // not is a legacy plaintext (the migration path).
                        config.password = Some(
                            self.open_password(&password).unwrap_or(password),
                        );
                    }
                    // T10: no key, no honest plaintext — the connection
                    // names its unavailability rather than passing the
                    // ciphertext through as a password.
                    Err(error) => {
                        return Err(ConnectionRepositoryError::BackendUnavailable(format!(
                            "the password of '{}' is unavailable ({error})",
                            config.name
                        )))
                    }
                }
            }
        }
        Ok(Some(config))
    }

    async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        if scope.organization_id() != datazen_ui_api::DESKTOP_ORGANIZATION {
            return Err(scope_error());
        }
        // T10: the password is sealed before it reaches the file; the file
        // never carries a plaintext password again.
        let mut config = config;
        if config.password.as_deref().is_some_and(|password| !password.is_empty()) {
            config.password = Some(self.seal_password(config.password.as_deref().unwrap())?);
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
        // The store tests opt into the file key backend: the OS keychain is
        // unreachable from a test environment without a session bus (the
        // desktop's own CI discipline, ).
        std::env::set_var("DATAZEN_NATIVE_KEYRING", "file");
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

    #[test]
    fn passwords_seal_on_save_and_reveal_on_get() {
        let dir = std::env::temp_dir().join(format!(
            "datazen-rune-store-seal-{}",
            std::process::id()
        ));
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
        assert!(!on_disk.contains("hunter2"), "the plaintext never lands: {on_disk}");
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
            listed.iter().find(|c| c.id == "secret").and_then(|c| c.password.clone()),
            Some("hunter2".into())
        );
        std::env::remove_var("DATAZEN_NATIVE_KEYRING");
    }

    #[test]
    fn an_encrypted_password_without_a_key_is_an_honest_unavailability() {
        // Write a sealed password with the file backend in one directory,
        // then move only the store to another: no key file, no keyring (this
        // sandbox has no session bus) — the reveal must fail closed with the
        // honest sentence, never pass the ciphertext through as a password.
        let dir = std::env::temp_dir().join(format!(
            "datazen-rune-store-closed-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::env::set_var("DATAZEN_NATIVE_KEYRING", "file");
        let write_dir = dir.join("write");
        std::fs::create_dir_all(&write_dir).unwrap();
        let scope = desktop_scope();
        let store = FileConnectionRepository::open(&write_dir.join("connections.json")).unwrap();
        let mut config = test_support::sqlite_config("sealed", None);
        config.password = Some("open-sesame".into());
        futures_executor::block_on(store.save(&scope, config)).unwrap();
        std::env::remove_var("DATAZEN_NATIVE_KEYRING");

        // Only the store moves; the key file stays behind.
        let read_dir = dir.join("read");
        std::fs::create_dir_all(&read_dir).unwrap();
        std::fs::copy(
            write_dir.join("connections.json"),
            read_dir.join("connections.json"),
        )
        .unwrap();

        let store = FileConnectionRepository::open(&read_dir.join("connections.json")).unwrap();
        // The keyring backend cannot be reached and no key file exists: the
        // reveal fails closed.
        let error = futures_executor::block_on(store.get(&scope, "sealed"))
            .expect_err("no key, no honest plaintext");
        assert!(
            error.to_string().contains("password of 'sealed' is unavailable"),
            "the sentence names the unavailability: {error}"
        );
        assert!(
            error.to_string().contains("DATAZEN_NATIVE_KEYRING=file"),
            "the sentence names the escape hatch: {error}"
        );
        // The listing stays whole: the password is absent, not leaked.
        let listed = futures_executor::block_on(store.list(&scope)).unwrap();
        let listed = listed.iter().find(|c| c.id == "sealed").unwrap();
        assert_eq!(listed.password, None, "no ciphertext in the listing");
    }
}
