//! Connection application use cases.
//!
//! Absorbs the business rules that used to live in the desktop command layer:
//! merging missing credentials on update (forms send redacted secrets back),
//! the redacted summary projection, and the duplicate flow. The repository is
//! injected; this layer never touches desktop paths, crypto, or the store.

use std::sync::Arc;

use datazen_driver_api::ConnectionConfig;

use crate::connection::domain::{ConnectionRepository, ConnectionRepositoryError, ConnectionScope};

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("connection not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Repository(#[from] ConnectionRepositoryError),
}

/// Strip credentials for summary/list projections.
///
/// Pure projection over a clone — the stored config keeps its secrets; only
/// the returned copy is redacted. Recursive over SSH jumps.
pub fn redact_secrets(config: &mut ConnectionConfig) {
    config.password = None;
    if let Some(ssh) = config.ssh_tunnel.as_mut() {
        redact_ssh_secrets(ssh);
    }
    if let Some(proxy) = config.http_proxy_tunnel.as_mut() {
        proxy.password = None;
        proxy.headers = None;
    }
    if let Some(websocket) = config.websocket_tunnel.as_mut() {
        websocket.auth_token = None;
        websocket.headers = None;
    }
}

fn redact_ssh_secrets(ssh: &mut datazen_driver_api::SshTunnelConfig) {
    ssh.password = None;
    ssh.passphrase = None;
    if let Some(jump) = ssh.jump.as_deref_mut() {
        redact_ssh_secrets(jump);
    }
}

/// Merge credentials the (redacted) form left empty back from the saved copy.
///
/// The connection form echoes secrets as `None`; an update that keeps the
/// existing password must not overwrite it with nothing. `None` on a *new*
/// connection is legal — there is no saved copy to merge from.
pub fn merge_missing_secrets(config: &mut ConnectionConfig, saved: ConnectionConfig) {
    if config.password.is_none() {
        config.password = saved.password;
    }
    if let (Some(current), Some(previous)) = (config.ssh_tunnel.as_mut(), saved.ssh_tunnel) {
        merge_missing_ssh_secrets(current, previous);
    }
    if let (Some(current), Some(previous)) =
        (config.http_proxy_tunnel.as_mut(), saved.http_proxy_tunnel)
    {
        if current.password.is_none() {
            current.password = previous.password;
        }
        if current.headers.is_none() {
            current.headers = previous.headers;
        }
    }
    if let (Some(current), Some(previous)) =
        (config.websocket_tunnel.as_mut(), saved.websocket_tunnel)
    {
        if current.auth_token.is_none() {
            current.auth_token = previous.auth_token;
        }
        if current.headers.is_none() {
            current.headers = previous.headers;
        }
    }
}

fn merge_missing_ssh_secrets(
    current: &mut datazen_driver_api::SshTunnelConfig,
    saved: datazen_driver_api::SshTunnelConfig,
) {
    if current.password.is_none() {
        current.password = saved.password;
    }
    if current.passphrase.is_none() {
        current.passphrase = saved.passphrase;
    }
    if let (Some(current_jump), Some(saved_jump)) = (current.jump.as_deref_mut(), saved.jump) {
        merge_missing_ssh_secrets(current_jump, *saved_jump);
    }
}

/// Connection catalog use cases: CRUD, credential merge, redacted summaries,
/// groups, duplicate.
pub struct ConnectionService {
    repository: Arc<dyn ConnectionRepository>,
}

impl ConnectionService {
    pub fn new(repository: Arc<dyn ConnectionRepository>) -> Self {
        Self { repository }
    }

    /// All configs with plaintext credentials — export / import paths that
    /// re-encrypt the whole catalog. Presentation paths must use
    /// [`ConnectionService::list_redacted`] instead.
    pub async fn list_plain(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionError> {
        Ok(self.repository.list(scope).await?)
    }

    /// All configs, redacted for presentation. The full plaintext config is
    /// only ever returned by [`ConnectionService::get`] on the connect path.
    pub async fn list_redacted(
        &self,
        scope: &ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, ConnectionError> {
        let mut list = self.repository.list(scope).await?;
        for config in &mut list {
            redact_secrets(config);
        }
        Ok(list)
    }

    /// One config including plaintext credentials (connect / test paths).
    pub async fn get(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, ConnectionError> {
        Ok(self.repository.get(scope, connection_id).await?)
    }

    /// Save with the form's credential-merge semantics.
    ///
    /// `clear_missing_credentials = true` writes the config verbatim (the user
    /// explicitly cleared secrets); otherwise empty secret fields are merged
    /// back from the saved copy so a redacted echo cannot blank the store.
    pub async fn save_merged(
        &self,
        scope: &ConnectionScope,
        mut config: ConnectionConfig,
        clear_missing_credentials: bool,
    ) -> Result<(), ConnectionError> {
        if !clear_missing_credentials {
            if let Some(saved) = self.repository.get(scope, &config.id).await? {
                merge_missing_secrets(&mut config, saved);
            }
        }
        self.repository.save(scope, config).await?;
        Ok(())
    }

    /// Save verbatim (import paths that already hold full configs).
    pub async fn save(
        &self,
        scope: &ConnectionScope,
        config: ConnectionConfig,
    ) -> Result<(), ConnectionError> {
        Ok(self.repository.save(scope, config).await?)
    }

    /// Duplicate a config under a new id and name, without the connect
    /// timestamp of the source.
    pub async fn duplicate(
        &self,
        scope: &ConnectionScope,
        source_id: &str,
        new_id: &str,
        new_name: &str,
    ) -> Result<(), ConnectionError> {
        let mut config = self
            .repository
            .get(scope, source_id)
            .await?
            .ok_or_else(|| ConnectionError::NotFound(source_id.to_string()))?;
        config.id = new_id.to_string();
        config.name = new_name.to_string();
        config.last_connected_at = None;
        self.repository.save(scope, config).await?;
        Ok(())
    }

    pub async fn delete(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionError> {
        Ok(self.repository.delete(scope, connection_id).await?)
    }

    pub async fn reorder(
        &self,
        scope: &ConnectionScope,
        ordered_ids: Vec<String>,
    ) -> Result<(), ConnectionError> {
        Ok(self.repository.reorder(scope, ordered_ids).await?)
    }

    /// Best-effort activity stamp: a failure is observational, not fatal —
    /// callers log and continue (the connect flow must not fail because a
    /// timestamp could not be written).
    pub async fn touch_last_connected_at(
        &self,
        scope: &ConnectionScope,
        connection_id: &str,
    ) -> Result<(), ConnectionError> {
        Ok(self
            .repository
            .touch_last_connected_at(scope, connection_id)
            .await?)
    }

    pub async fn groups(&self, scope: &ConnectionScope) -> Result<Vec<String>, ConnectionError> {
        Ok(self.repository.groups(scope).await?)
    }

    pub async fn save_groups(
        &self,
        scope: &ConnectionScope,
        groups: Vec<String>,
    ) -> Result<(), ConnectionError> {
        Ok(self.repository.save_groups(scope, groups).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use datazen_driver_api::{ConnectionConfig, SshTunnelConfig};
    use std::sync::Mutex;

    struct MemoryRepository {
        connections: Mutex<Vec<ConnectionConfig>>,
        groups: Mutex<Vec<String>>,
    }

    impl MemoryRepository {
        fn new() -> Self {
            Self {
                connections: Mutex::new(Vec::new()),
                groups: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ConnectionRepository for MemoryRepository {
        async fn list(
            &self,
            _scope: &ConnectionScope,
        ) -> Result<Vec<ConnectionConfig>, ConnectionRepositoryError> {
            Ok(self.connections.lock().unwrap().clone())
        }

        async fn get(
            &self,
            _scope: &ConnectionScope,
            connection_id: &str,
        ) -> Result<Option<ConnectionConfig>, ConnectionRepositoryError> {
            Ok(self
                .connections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == connection_id)
                .cloned())
        }

        async fn save(
            &self,
            _scope: &ConnectionScope,
            config: ConnectionConfig,
        ) -> Result<(), ConnectionRepositoryError> {
            let mut connections = self.connections.lock().unwrap();
            match connections.iter().position(|c| c.id == config.id) {
                Some(pos) => connections[pos] = config,
                None => connections.push(config),
            }
            Ok(())
        }

        async fn delete(
            &self,
            _scope: &ConnectionScope,
            connection_id: &str,
        ) -> Result<(), ConnectionRepositoryError> {
            self.connections
                .lock()
                .unwrap()
                .retain(|c| c.id != connection_id);
            Ok(())
        }

        async fn reorder(
            &self,
            _scope: &ConnectionScope,
            ordered_ids: Vec<String>,
        ) -> Result<(), ConnectionRepositoryError> {
            let mut connections = self.connections.lock().unwrap();
            let mut by_id: std::collections::HashMap<String, ConnectionConfig> = connections
                .drain(..)
                .map(|c| (c.id.clone(), c))
                .collect();
            for id in &ordered_ids {
                if let Some(c) = by_id.remove(id) {
                    connections.push(c);
                }
            }
            let mut remaining: Vec<ConnectionConfig> = by_id.into_values().collect();
            remaining.sort_by(|a, b| a.name.cmp(&b.name));
            connections.extend(remaining);
            Ok(())
        }

        async fn touch_last_connected_at(
            &self,
            _scope: &ConnectionScope,
            connection_id: &str,
        ) -> Result<(), ConnectionRepositoryError> {
            let mut connections = self.connections.lock().unwrap();
            if let Some(config) = connections.iter_mut().find(|c| c.id == connection_id) {
                config.last_connected_at = Some("2026-10-26T00:00:00Z".into());
            }
            Ok(())
        }

        async fn groups(
            &self,
            _scope: &ConnectionScope,
        ) -> Result<Vec<String>, ConnectionRepositoryError> {
            Ok(self.groups.lock().unwrap().clone())
        }

        async fn save_groups(
            &self,
            _scope: &ConnectionScope,
            groups: Vec<String>,
        ) -> Result<(), ConnectionRepositoryError> {
            *self.groups.lock().unwrap() = groups;
            Ok(())
        }
    }

    fn sample(id: &str, password: Option<&str>) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: format!("connection {id}"),
            database_type: "postgresql".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            username: Some("user".into()),
            password: password.map(Into::into),
            schema: None,
            ssl_mode: datazen_driver_api::SslMode::Prefer,
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

    fn scope() -> ConnectionScope {
        ConnectionScope::new("org-1")
    }

    #[tokio::test]
    async fn save_merged_keeps_saved_secret_when_form_sends_none() {
        let repository = Arc::new(MemoryRepository::new());
        let service = ConnectionService::new(repository.clone());
        service
            .save(&scope(), sample("c1", Some("hunter2")))
            .await
            .expect("seed save");

        // The form echoes the secret back as None (redacted display).
        let mut echoed = sample("c1", None);
        echoed.username = Some("user".into());
        service
            .save_merged(&scope(), echoed, false)
            .await
            .expect("merge save");

        let stored = service.get(&scope(), "c1").await.unwrap().expect("stored");
        assert_eq!(stored.password.as_deref(), Some("hunter2"));
    }

    #[tokio::test]
    async fn save_merged_clears_secret_when_explicitly_requested() {
        let repository = Arc::new(MemoryRepository::new());
        let service = ConnectionService::new(repository);
        service
            .save(&scope(), sample("c1", Some("hunter2")))
            .await
            .unwrap();
        service
            .save_merged(&scope(), sample("c1", None), true)
            .await
            .unwrap();
        let stored = service.get(&scope(), "c1").await.unwrap().unwrap();
        assert_eq!(stored.password, None);
    }

    #[tokio::test]
    async fn list_redacted_strips_every_secret_including_nested_tunnels() {
        let repository = Arc::new(MemoryRepository::new());
        let service = ConnectionService::new(repository.clone());
        let mut config = sample("c1", Some("hunter2"));
        config.ssh_tunnel = Some(SshTunnelConfig {
            enabled: true,
            host: "bastion".into(),
            port: 22,
            username: "ops".into(),
            auth_method: "password".into(),
            password: Some("sshpass".into()),
            private_key_path: None,
            passphrase: None,
            jump: None,
        });
        service.save(&scope(), config).await.unwrap();

        let listed = service.list_redacted(&scope()).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].password, None);
        assert_eq!(
            listed[0].ssh_tunnel.as_ref().and_then(|s| s.password.clone()),
            None
        );
        // The stored copy keeps the real secrets for the connect path.
        let stored = service.get(&scope(), "c1").await.unwrap().unwrap();
        assert_eq!(stored.password.as_deref(), Some("hunter2"));
    }

    #[tokio::test]
    async fn duplicate_resets_identity_and_timestamp() {
        let repository = Arc::new(MemoryRepository::new());
        let service = ConnectionService::new(repository.clone());
        let mut config = sample("c1", Some("hunter2"));
        config.last_connected_at = Some("2025-01-01T00:00:00Z".into());
        service.save(&scope(), config).await.unwrap();

        service
            .duplicate(&scope(), "c1", "c2", "copy of c1")
            .await
            .expect("duplicate");

        let copy = service.get(&scope(), "c2").await.unwrap().unwrap();
        assert_eq!(copy.name, "copy of c1");
        assert_eq!(copy.password.as_deref(), Some("hunter2"));
        assert_eq!(copy.last_connected_at, None);
    }

    #[tokio::test]
    async fn duplicate_of_missing_source_reports_not_found() {
        let service = ConnectionService::new(Arc::new(MemoryRepository::new()));
        let error = service
            .duplicate(&scope(), "missing", "c2", "x")
            .await
            .expect_err("source must exist");
        assert!(matches!(error, ConnectionError::NotFound(id) if id == "missing"));
    }

    #[tokio::test]
    async fn reorder_keeps_unmentioned_ids_at_the_end() {
        let repository = Arc::new(MemoryRepository::new());
        let service = ConnectionService::new(repository.clone());
        for id in ["a", "b", "c"] {
            service.save(&scope(), sample(id, None)).await.unwrap();
        }
        service
            .reorder(&scope(), vec!["c".into(), "a".into()])
            .await
            .unwrap();
        let order: Vec<String> = service
            .list_redacted(&scope())
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(order, vec!["c", "a", "b"]);
    }
}
