//! Independent tunnel definitions (`tunnels.json`).
//!
//! Connections reference a tunnel via `ConnectionConfig.tunnel_id`.
//! Sensitive fields (SSH password/passphrase, HTTP proxy password, WS auth token)
//! are encrypted at rest with the same AES-256-GCM key as connection passwords.

use crate::db::SavedTunnel;

use super::{Store, StoreError};

impl Store {
    pub(super) async fn load_tunnels_from_disk(&self) -> Result<Vec<SavedTunnel>, StoreError> {
        let path = self.data_dir.join("tunnels.json");
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| StoreError::ReadError(e.to_string()))?;

        let mut tunnels: Vec<SavedTunnel> =
            serde_json::from_str(&content).map_err(|e| StoreError::ParseError(e.to_string()))?;

        for t in &mut tunnels {
            self.decrypt_tunnel_secrets(t);
        }

        Ok(tunnels)
    }

    fn decrypt_tunnel_secrets(&self, t: &mut SavedTunnel) {
        if let Some(ref mut ssh) = t.ssh {
            if let Some(enc) = &ssh.password {
                match self.decrypt(enc) {
                    Ok(plain) => ssh.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt SSH password");
                        ssh.password = None;
                    }
                }
            }
            if let Some(enc) = &ssh.passphrase {
                match self.decrypt(enc) {
                    Ok(plain) => ssh.passphrase = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt SSH passphrase");
                        ssh.passphrase = None;
                    }
                }
            }
            if let Some(ref mut jump) = ssh.jump {
                // Nested ProxyJump hop — only one level of secrets for now.
                if let Some(enc) = &jump.password {
                    match self.decrypt(enc) {
                        Ok(plain) => jump.password = Some(plain),
                        Err(_) => jump.password = None,
                    }
                }
                if let Some(enc) = &jump.passphrase {
                    match self.decrypt(enc) {
                        Ok(plain) => jump.passphrase = Some(plain),
                        Err(_) => jump.passphrase = None,
                    }
                }
            }
        }
        if let Some(ref mut http) = t.http_proxy {
            if let Some(enc) = &http.password {
                match self.decrypt(enc) {
                    Ok(plain) => http.password = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt HTTP proxy password");
                        http.password = None;
                    }
                }
            }
        }
        if let Some(ref mut ws) = t.websocket {
            if let Some(enc) = &ws.auth_token {
                match self.decrypt(enc) {
                    Ok(plain) => ws.auth_token = Some(plain),
                    Err(e) => {
                        tracing::warn!(tunnel = %t.name, error = %e, "Failed to decrypt WS auth token");
                        ws.auth_token = None;
                    }
                }
            }
        }
    }

    fn encrypt_tunnel_secrets(&self, t: &SavedTunnel) -> Result<SavedTunnel, StoreError> {
        let mut out = t.clone();
        if let Some(ref mut ssh) = out.ssh {
            if let Some(pw) = &ssh.password {
                if !pw.is_empty() {
                    ssh.password = Some(self.encrypt(pw)?);
                }
            }
            if let Some(pp) = &ssh.passphrase {
                if !pp.is_empty() {
                    ssh.passphrase = Some(self.encrypt(pp)?);
                }
            }
            if let Some(ref mut jump) = ssh.jump {
                if let Some(pw) = &jump.password {
                    if !pw.is_empty() {
                        jump.password = Some(self.encrypt(pw)?);
                    }
                }
                if let Some(pp) = &jump.passphrase {
                    if !pp.is_empty() {
                        jump.passphrase = Some(self.encrypt(pp)?);
                    }
                }
            }
        }
        if let Some(ref mut http) = out.http_proxy {
            if let Some(pw) = &http.password {
                if !pw.is_empty() {
                    http.password = Some(self.encrypt(pw)?);
                }
            }
        }
        if let Some(ref mut ws) = out.websocket {
            if let Some(tok) = &ws.auth_token {
                if !tok.is_empty() {
                    ws.auth_token = Some(self.encrypt(tok)?);
                }
            }
        }
        Ok(out)
    }

    async fn persist_tunnels(&self, tunnels: &[SavedTunnel]) -> Result<(), StoreError> {
        let mut to_disk = Vec::with_capacity(tunnels.len());
        for t in tunnels {
            to_disk.push(self.encrypt_tunnel_secrets(t)?);
        }
        self.save_json_file("tunnels.json", &to_disk).await
    }

    pub async fn get_tunnels(&self) -> Vec<SavedTunnel> {
        let cache = self.cache.read().await;
        cache.tunnels.clone()
    }

    pub async fn get_tunnel(&self, id: &str) -> Option<SavedTunnel> {
        let cache = self.cache.read().await;
        cache.tunnels.iter().find(|t| t.id == id).cloned()
    }

    pub async fn save_tunnel(&self, tunnel: SavedTunnel) -> Result<(), StoreError> {
        let _guard = self.write_lock.lock().await;

        {
            let mut cache = self.cache.write().await;
            if let Some(pos) = cache.tunnels.iter().position(|t| t.id == tunnel.id) {
                cache.tunnels[pos] = tunnel;
            } else {
                cache.tunnels.push(tunnel);
            }
        }

        let snapshot = {
            let cache = self.cache.read().await;
            cache.tunnels.clone()
        };

        // Already holding write_lock — encrypt + atomic write without re-locking.
        let mut to_disk = Vec::with_capacity(snapshot.len());
        for t in &snapshot {
            to_disk.push(self.encrypt_tunnel_secrets(t)?);
        }
        let content = serde_json::to_string_pretty(&to_disk)
            .map_err(|e| StoreError::ParseError(e.to_string()))?;
        let path = self.data_dir.join("tunnels.json");
        Self::write_file_atomic(&path, content).await
    }

    pub async fn delete_tunnel(&self, id: &str) -> Result<(), StoreError> {
        {
            let mut cache = self.cache.write().await;
            cache.tunnels.retain(|t| t.id != id);
        }
        let snapshot = {
            let cache = self.cache.read().await;
            cache.tunnels.clone()
        };
        self.persist_tunnels(&snapshot).await
    }
}
