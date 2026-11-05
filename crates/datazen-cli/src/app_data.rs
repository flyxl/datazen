//! Read-only adapter over the desktop application data directory.
//!
//! DataZen desktop persists profiles in `{appData}/connections.json`; this
//! adapter implements the shared [`ConnectionRepository`] port so the CLI and
//! the desktop use the same vocabulary. It never writes profiles.
//!
//! Secrets in that file are AES-256-GCM ciphertext. The CLI keeps them opaque
//! for listings; only a connect path asks for plaintext, and a missing key is a
//! stable `E_CREDENTIAL` error instead of passing ciphertext to a driver.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use datazen::connection::domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope,
};
use datazen::connection::ConnectionService;
use datazen_driver_api::ConnectionConfig;
use serde_json::Value;

use crate::error::{CliError, E_CONFIG, E_CREDENTIAL};

pub const APP_IDENTIFIER: &str = "com.tbeasy.datazen";
pub const APP_DATA_ENV: &str = "DATAZEN_DATA_DIR";
const CONNECTIONS_FILE: &str = "connections.json";
const KEY_FILE: &str = ".key";
const KEYRING_ACCOUNT: &str = "app-encryption-key";

pub fn default_data_dir() -> Result<PathBuf, CliError> {
    if let Some(dir) = std::env::var_os(APP_DATA_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    dirs::data_dir()
        .map(|dir| dir.join(APP_IDENTIFIER))
        .ok_or_else(|| CliError::new(E_CONFIG, "cannot determine the DataZen app data directory"))
}

pub fn connections_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CONNECTIONS_FILE)
}

#[derive(Debug, Clone)]
pub struct AppDataStore {
    data_dir: PathBuf,
}

impl AppDataStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn connections_path(&self) -> PathBuf {
        connections_path(&self.data_dir)
    }

    pub fn key_file_exists(&self) -> bool {
        self.data_dir.join(KEY_FILE).is_file()
    }

    pub fn key_backend(&self) -> &'static str {
        match std::env::var("DATAZEN_KEYRING").ok().as_deref() {
            Some("keyring") => "keyring",
            Some("file") => {
                if self.key_file_exists() {
                    "file"
                } else {
                    "unavailable"
                }
            }
            _ if self.key_file_exists() => "file",
            _ => auto_key_backend(),
        }
    }

    pub fn service(&self) -> ConnectionService {
        ConnectionService::new(Arc::new(self.clone()) as Arc<dyn ConnectionRepository>)
    }

    pub async fn list_redacted(&self) -> Result<Vec<ConnectionConfig>, CliError> {
        let service = self.service();
        let scope = ConnectionScope::new("cli");
        service
            .list_redacted(&scope)
            .await
            .map_err(connection_error)
    }

    pub async fn get_plain(&self, id: &str) -> Result<ConnectionConfig, CliError> {
        let service = self.service();
        let scope = ConnectionScope::new("cli");
        service
            .get(&scope, id)
            .await
            .map_err(connection_error)?
            .ok_or_else(|| {
                CliError::new(
                    crate::error::E_NOT_FOUND,
                    format!("connection '{id}' not found"),
                )
            })
    }

    fn read_records(&self) -> Result<Vec<Value>, CliError> {
        let path = self.connections_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = std::fs::read(&path).map_err(|error| {
            CliError::new(E_CONFIG, format!("cannot read connections: {error}"))
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| CliError::new(E_CONFIG, format!("cannot parse connections: {error}")))
    }

    fn read_configs(&self) -> Result<Vec<ConnectionConfig>, CliError> {
        self.read_records()?
            .into_iter()
            .map(|value| {
                serde_json::from_value(value).map_err(|error| {
                    CliError::new(E_CONFIG, format!("invalid connection: {error}"))
                })
            })
            .collect()
    }

    fn decrypted(&self, mut config: ConnectionConfig) -> Result<ConnectionConfig, CliError> {
        if !has_encrypted_secret(&config) {
            return Ok(config);
        }
        let key = self.encryption_key()?;
        if let Some(password) = config.password.take() {
            config.password = Some(decrypt_secret(&key, &password)?);
        }
        if let Some(ssh) = config.ssh_tunnel.as_mut() {
            decrypt_ssh(&key, ssh)?;
        }
        Ok(config)
    }

    fn encryption_key(&self) -> Result<[u8; 32], CliError> {
        match std::env::var("DATAZEN_KEYRING").ok().as_deref() {
            Some("keyring") => return read_keyring_key(),
            Some("file") => return read_key_file(&self.data_dir),
            _ => {}
        }
        if self.key_file_exists() {
            return read_key_file(&self.data_dir);
        }
        if crate::platform_vault::preferred() {
            match crate::platform_vault::load().map_err(|error| {
                CliError::new(
                    E_CREDENTIAL,
                    format!("cannot read the platform vault: {error}"),
                )
            })? {
                Some(key) => return Ok(key),
                None if cfg!(target_os = "windows") => {
                    return Err(CliError::new(
                        E_CREDENTIAL,
                        "credentials are encrypted but the DPAPI vault has no DataZen master key",
                    ));
                }
                None => {}
            }
        }
        read_keyring_key()
    }
}

fn auto_key_backend() -> &'static str {
    if crate::platform_vault::preferred() {
        "platform-vault"
    } else {
        "keyring"
    }
}

fn has_encrypted_secret(config: &ConnectionConfig) -> bool {
    config.password.as_deref().is_some_and(looks_encrypted)
        || config
            .ssh_tunnel
            .as_ref()
            .is_some_and(ssh_has_encrypted_secret)
}

fn ssh_has_encrypted_secret(ssh: &datazen_driver_api::SshTunnelConfig) -> bool {
    ssh.password.as_deref().is_some_and(looks_encrypted)
        || ssh.passphrase.as_deref().is_some_and(looks_encrypted)
        || ssh.jump.as_deref().is_some_and(ssh_has_encrypted_secret)
}

fn looks_encrypted(value: &str) -> bool {
    BASE64
        .decode(value)
        .map(|bytes| bytes.len() >= 12)
        .unwrap_or(false)
}

fn decrypt_ssh(
    key: &[u8; 32],
    ssh: &mut datazen_driver_api::SshTunnelConfig,
) -> Result<(), CliError> {
    if let Some(password) = ssh.password.take() {
        ssh.password = Some(decrypt_secret(key, &password)?);
    }
    if let Some(passphrase) = ssh.passphrase.take() {
        ssh.passphrase = Some(decrypt_secret(key, &passphrase)?);
    }
    if let Some(jump) = ssh.jump.as_deref_mut() {
        decrypt_ssh(key, jump)?;
    }
    Ok(())
}

fn read_key_file(data_dir: &Path) -> Result<[u8; 32], CliError> {
    let path = data_dir.join(KEY_FILE);
    let encoded = std::fs::read_to_string(&path).map_err(|error| {
        CliError::new(
            E_CREDENTIAL,
            format!("cannot read the app encryption key: {error}"),
        )
    })?;
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| CliError::new(E_CREDENTIAL, "the app encryption key is not valid base64"))?;
    bytes
        .try_into()
        .map_err(|_| CliError::new(E_CREDENTIAL, "the app encryption key has an invalid length"))
}

fn read_keyring_key() -> Result<[u8; 32], CliError> {
    let entry = keyring::v1::Entry::new(APP_IDENTIFIER, KEYRING_ACCOUNT).map_err(|error| {
        CliError::new(
            E_CREDENTIAL,
            format!("cannot open the OS keychain entry: {error}"),
        )
    })?;
    let encoded = entry.get_password().map_err(|error| {
        CliError::new(
            E_CREDENTIAL,
            format!("cannot read the app encryption key from the OS keychain: {error}"),
        )
    })?;
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|_| CliError::new(E_CREDENTIAL, "the keychain key is not valid base64"))?;
    bytes
        .try_into()
        .map_err(|_| CliError::new(E_CREDENTIAL, "the keychain key has an invalid length"))
}

/// Decrypt a stored secret. A value that is not shaped like one of our
/// AES-GCM payloads is treated as already-plaintext (imported fixtures); a
/// well-formed ciphertext that cannot be opened is a credentials error.
fn decrypt_secret(key: &[u8; 32], value: &str) -> Result<String, CliError> {
    let Ok(combined) = BASE64.decode(value) else {
        return Ok(value.to_string());
    };
    if combined.len() < 12 {
        return Ok(value.to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CliError::new(E_CREDENTIAL, "invalid encryption key"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| {
            CliError::new(
                E_CREDENTIAL,
                "cannot decrypt stored credentials with the available key",
            )
        })?;
    String::from_utf8(plaintext)
        .map_err(|_| CliError::new(E_CREDENTIAL, "decrypted credentials are not UTF-8"))
}

#[async_trait]
impl ConnectionRepository for AppDataStore {
    async fn list(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
        self.read_configs()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.message))
    }

    async fn get(
        &self,
        _scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
        let Some(config) = self
            .read_configs()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.message))?
            .into_iter()
            .find(|config| config.id == connection_id)
        else {
            return Ok(None);
        };
        self.decrypted(config)
            .map(Some)
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.message))
    }

    async fn save(
        &self,
        _scope: &ConnectionScope,
        _config: ConnectionConfig,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "the DataZen CLI is read-only for profiles".into(),
        ))
    }

    async fn delete(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "the DataZen CLI is read-only for profiles".into(),
        ))
    }

    async fn reorder(
        &self,
        _scope: &ConnectionScope,
        _ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "the DataZen CLI is read-only for profiles".into(),
        ))
    }

    async fn touch_last_connected_at(
        &self,
        _scope: &ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), ConnectionRepositoryError> {
        Ok(())
    }

    async fn groups(
        &self,
        _scope: &ConnectionScope,
    ) -> Result<Vec<String>, ConnectionRepositoryError> {
        let mut groups: Vec<String> = self
            .read_configs()
            .map_err(|error| ConnectionRepositoryError::BackendUnavailable(error.message))?
            .into_iter()
            .filter_map(|config| config.group)
            .collect();
        groups.sort();
        groups.dedup();
        Ok(groups)
    }

    async fn save_groups(
        &self,
        _scope: &ConnectionScope,
        _groups: Vec<String>,
    ) -> Result<(), ConnectionRepositoryError> {
        Err(ConnectionRepositoryError::BackendUnavailable(
            "the DataZen CLI is read-only for profile groups".into(),
        ))
    }
}

fn connection_error(error: datazen::connection::ConnectionError) -> CliError {
    match error {
        datazen::connection::ConnectionError::NotFound(id) => CliError::new(
            crate::error::E_NOT_FOUND,
            format!("connection '{id}' not found"),
        ),
        other => {
            let message = other.to_string();
            let code = if message.contains("credential")
                || message.contains("encrypt")
                || message.contains("decrypt")
                || message.contains("keychain")
            {
                E_CREDENTIAL
            } else {
                E_CONFIG
            };
            CliError::new(code, message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_connections(dir: &Path, value: Value) {
        std::fs::write(connections_path(dir), serde_json::to_vec(&value).unwrap()).unwrap();
    }

    #[test]
    fn missing_connections_file_is_an_empty_catalog() {
        let temp = tempfile::tempdir().unwrap();
        let store = AppDataStore::new(temp.path().to_path_buf());
        assert!(store.read_configs().unwrap().is_empty());
    }

    #[test]
    fn malformed_connections_file_has_a_stable_config_error() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(connections_path(temp.path()), b"not json").unwrap();
        let store = AppDataStore::new(temp.path().to_path_buf());
        assert_eq!(store.read_configs().unwrap_err().code, E_CONFIG);
    }

    #[tokio::test]
    async fn key_file_decrypts_a_stored_secret_without_listing_it() {
        use aes_gcm::aead::{Aead, KeyInit};
        let temp = tempfile::tempdir().unwrap();
        let key = [7_u8; 32];
        std::fs::write(temp.path().join(KEY_FILE), BASE64.encode(key)).unwrap();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let nonce = Nonce::from_slice(b"123456789012");
        let encrypted = cipher.encrypt(nonce, b"db-password".as_slice()).unwrap();
        let mut combined = nonce.to_vec();
        combined.extend(encrypted);
        write_connections(
            temp.path(),
            serde_json::json!([{
                "id": "one",
                "name": "One",
                "databaseType": "sqlite",
                "password": BASE64.encode(combined),
                "sshTunnel": null,
                "httpProxyTunnel": null,
                "websocketTunnel": null
            }]),
        );
        let store = AppDataStore::new(temp.path().to_path_buf());
        let listed = store.list_redacted().await.unwrap();
        assert_eq!(listed[0].password, None);
        let config = store.get_plain("one").await.unwrap();
        assert_eq!(config.password.as_deref(), Some("db-password"));
    }

    #[test]
    fn plaintext_imported_secret_is_not_mistaken_for_ciphertext() {
        let temp = tempfile::tempdir().unwrap();
        let store = AppDataStore::new(temp.path().to_path_buf());
        write_connections(
            temp.path(),
            serde_json::json!([{
                "id": "one",
                "name": "One",
                "databaseType": "sqlite",
                "password": "secret",
                "sshTunnel": null,
                "httpProxyTunnel": null,
                "websocketTunnel": null
            }]),
        );
        let config = store.read_configs().unwrap().remove(0);
        let decrypted = store.decrypted(config).unwrap();
        assert_eq!(decrypted.password.as_deref(), Some("secret"));
    }
}
