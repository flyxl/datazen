//! SSH tunnel for forwarding database connections through an SSH jump host.
//!
//! When `SshTunnelConfig.enabled` is true, we:
//! 1. Open an SSH session to the jump host
//! 2. Authenticate (password or private key)
//! 3. Bind a local TCP listener on 127.0.0.1:<random>
//! 4. For every incoming local connection, open a `direct-tcpip` channel
//!    through the SSH session and pipe data bidirectionally.
//! 5. Return the local port so DB drivers can connect to 127.0.0.1:<local>.

use crate::db::{DriverError, SshTunnelConfig};
use crate::ssh_known_hosts::{
    host_key_id, known_host_entry_from_key, load_known_hosts, mismatch_error_message,
    save_known_hosts, verify_host_key, HostKeyDecision, KnownHostEntry,
};
use russh::client::{self, AuthResult};
use russh::keys::{self, ssh_key, PrivateKeyWithHashAlg};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

// ── SSH client handler (TOFU host key verification) ─────────────────

struct TunnelHandler {
    host_id: String,
    known_hosts_path: std::path::PathBuf,
    known_hosts: Arc<Mutex<HashMap<String, KnownHostEntry>>>,
    rejection: Arc<Mutex<Option<String>>>,
}

impl client::Handler for TunnelHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        key: &ssh_key::PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let host_id = self.host_id.clone();
        let known_hosts_path = self.known_hosts_path.clone();
        let known_hosts = self.known_hosts.clone();
        let rejection = self.rejection.clone();

        async move {
            let observed = match known_host_entry_from_key(key) {
                Ok(entry) => entry,
                Err(e) => {
                    if let Ok(mut guard) = rejection.lock() {
                        *guard = Some(format!(
                            "SSH host key verification failed for {host_id}: {e}"
                        ));
                    }
                    return Ok(false);
                }
            };

            let mut map = match known_hosts.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::warn!(host = %host_id, "SSH known_hosts lock poisoned; recovering");
                    poisoned.into_inner()
                }
            };
            match verify_host_key(map.get(&host_id), &observed) {
                HostKeyDecision::AcceptMatch => Ok(true),
                HostKeyDecision::AcceptFirstUse { fingerprint } => {
                    tracing::info!(
                        host = %host_id,
                        fingerprint = %fingerprint,
                        algorithm = %observed.algorithm,
                        "SSH host key accepted (TOFU)"
                    );
                    map.insert(host_id, observed);
                    if let Err(e) = save_known_hosts(&known_hosts_path, &map) {
                        tracing::warn!(error = %e, "Failed to persist SSH known host");
                    }
                    Ok(true)
                }
                HostKeyDecision::RejectMismatch { expected, received } => {
                    tracing::warn!(
                        host = %host_id,
                        expected = %expected,
                        received = %received,
                        "SSH host key mismatch"
                    );
                    if let Ok(mut guard) = rejection.lock() {
                        *guard = Some(mismatch_error_message(&host_id, &expected, &received));
                    }
                    Ok(false)
                }
            }
        }
    }
}

// ── Public tunnel struct ────────────────────────────────────────────

pub struct SshTunnel {
    local_port: u16,
    _task: tokio::task::JoinHandle<()>,
    _upstream: Option<Box<SshTunnel>>,
}

pub fn supported_auth_method(method: &str) -> bool {
    matches!(method, "password" | "private_key" | "agent")
}

impl SshTunnel {
    /// Establish an SSH tunnel that forwards `127.0.0.1:<local_port>` →
    /// `remote_host:remote_port` through the configured SSH jump host.
    pub async fn start(
        ssh: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
        known_hosts_path: &Path,
    ) -> Result<Self, DriverError> {
        if !supported_auth_method(&ssh.auth_method) {
            return Err(DriverError::SshTunnelError(format!(
                "Unknown SSH auth method: {}",
                ssh.auth_method
            )));
        }

        let upstream = match ssh.jump.as_deref() {
            Some(jump) if jump.enabled => Some(Box::new(
                Box::pin(SshTunnel::start(
                    jump,
                    &ssh.host,
                    ssh.port,
                    known_hosts_path,
                ))
                .await?,
            )),
            _ => None,
        };

        let (connect_host, connect_port): (String, u16) = if let Some(ref up) = upstream {
            ("127.0.0.1".into(), up.local_port())
        } else {
            (ssh.host.clone(), ssh.port)
        };

        let config = Arc::new(client::Config::default());
        let host_id = host_key_id(&ssh.host, ssh.port);
        let rejection = Arc::new(Mutex::new(None));
        let known_hosts = Arc::new(Mutex::new(load_known_hosts(known_hosts_path)));

        let handler = TunnelHandler {
            host_id: host_id.clone(),
            known_hosts_path: known_hosts_path.to_path_buf(),
            known_hosts,
            rejection: rejection.clone(),
        };

        // 1. Connect (possibly via an upstream jump tunnel bound on localhost)
        let mut session = client::connect(config, (connect_host.as_str(), connect_port), handler)
            .await
            .map_err(|e| {
                if let Ok(mut guard) = rejection.lock() {
                    if let Some(msg) = guard.take() {
                        return DriverError::SshTunnelError(msg);
                    }
                }
                DriverError::SshTunnelError(format!(
                    "SSH connect to {}:{} failed: {e}",
                    ssh.host, ssh.port
                ))
            })?;

        // 2. Authenticate
        authenticate_session(&mut session, ssh).await?;

        // 3. Bind local listener
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| DriverError::SshTunnelError(format!("Bind local port: {e}")))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| DriverError::SshTunnelError(format!("get local port: {e}")))?
            .port();

        tracing::info!(
            ssh_host = %ssh.host,
            ssh_port = ssh.port,
            local_port,
            remote = %format!("{remote_host}:{remote_port}"),
            "SSH tunnel established"
        );

        // 4. Spawn forwarding loop — accepts multiple concurrent connections
        let rh = remote_host.to_string();
        let session = Arc::new(tokio::sync::Mutex::new(session));
        let task = tokio::spawn(async move {
            loop {
                let accept = listener.accept().await;
                let (mut tcp_stream, _) = match accept {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("SSH tunnel accept error: {e}");
                        break;
                    }
                };

                let rh = rh.clone();
                let session = session.clone();
                let lp = local_port;

                tokio::spawn(async move {
                    let channel = {
                        let session = session.lock().await;
                        match session
                            .channel_open_direct_tcpip(
                                rh,
                                remote_port as u32,
                                "127.0.0.1",
                                lp as u32,
                            )
                            .await
                        {
                            Ok(ch) => ch,
                            Err(e) => {
                                tracing::error!("SSH direct-tcpip channel: {e}");
                                return;
                            }
                        }
                    };
                    let mut ssh_stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut tcp_stream, &mut ssh_stream).await;
                });
            }
        });

        Ok(SshTunnel {
            local_port,
            _task: task,
            _upstream: upstream,
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}

async fn authenticate_session(
    session: &mut client::Handle<TunnelHandler>,
    ssh: &SshTunnelConfig,
) -> Result<(), DriverError> {
    match ssh.auth_method.as_str() {
        "password" => {
            let pw = ssh.password.as_deref().unwrap_or("");
            let result = session
                .authenticate_password(&ssh.username, pw)
                .await
                .map_err(|e| DriverError::SshTunnelError(format!("SSH password auth: {e}")))?;
            if !matches!(result, AuthResult::Success) {
                return Err(DriverError::SshTunnelError(
                    "SSH password authentication rejected".into(),
                ));
            }
        }
        "private_key" => {
            let key_path = ssh.private_key_path.as_deref().unwrap_or("~/.ssh/id_rsa");
            let expanded = expand_home(key_path);

            let secret_key =
                keys::load_secret_key(&expanded, ssh.passphrase.as_deref()).map_err(|e| {
                    DriverError::SshTunnelError(format!("Load SSH key {expanded}: {e}"))
                })?;

            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(secret_key), None);

            let result = session
                .authenticate_publickey(&ssh.username, key_with_hash)
                .await
                .map_err(|e| DriverError::SshTunnelError(format!("SSH key auth: {e}")))?;
            if !matches!(result, AuthResult::Success) {
                return Err(DriverError::SshTunnelError(
                    "SSH public key authentication rejected".into(),
                ));
            }
        }
        "agent" => authenticate_with_agent(session, ssh).await?,
        other => {
            return Err(DriverError::SshTunnelError(format!(
                "Unknown SSH auth method: {other}"
            )));
        }
    }
    Ok(())
}

async fn authenticate_with_agent(
    session: &mut client::Handle<TunnelHandler>,
    ssh: &SshTunnelConfig,
) -> Result<(), DriverError> {
    #[cfg(unix)]
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|e| DriverError::SshTunnelError(format!("SSH agent: {e}")))?;
    #[cfg(windows)]
    let mut agent = russh::keys::agent::client::AgentClient::connect_pageant()
        .await
        .map_err(|e| DriverError::SshTunnelError(format!("SSH agent: {e}")))?;
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (session, ssh);
        return Err(DriverError::SshTunnelError(
            "SSH agent is not supported on this platform".into(),
        ));
    }

    #[cfg(any(unix, windows))]
    {
        let identities = agent
            .request_identities()
            .await
            .map_err(|e| DriverError::SshTunnelError(format!("SSH agent identities: {e}")))?;
        if identities.is_empty() {
            return Err(DriverError::SshTunnelError(
                "SSH agent has no identities".into(),
            ));
        }
        let hash_alg = session
            .best_supported_rsa_hash()
            .await
            .ok()
            .flatten()
            .flatten();
        for identity in identities {
            let result = match identity {
                russh::keys::agent::AgentIdentity::PublicKey { key, .. } => {
                    let alg = match key.algorithm() {
                        russh::keys::Algorithm::Rsa { .. } | russh::keys::Algorithm::Dsa => {
                            hash_alg
                        }
                        _ => None,
                    };
                    session
                        .authenticate_publickey_with(&ssh.username, key, alg, &mut agent)
                        .await
                }
                russh::keys::agent::AgentIdentity::Certificate { certificate, .. } => {
                    session
                        .authenticate_certificate_with(
                            &ssh.username,
                            certificate,
                            hash_alg,
                            &mut agent,
                        )
                        .await
                }
            };
            match result {
                Ok(AuthResult::Success) => return Ok(()),
                Ok(_) => continue,
                Err(e) => {
                    tracing::debug!(error = %e, "SSH agent identity rejected");
                }
            }
        }
        Err(DriverError::SshTunnelError(
            "SSH agent authentication rejected".into(),
        ))
    }
}

fn expand_home(path: &str) -> String {
    if path.starts_with("~/") {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(|h| format!("{h}/{}", &path[2..]))
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_known_hosts::{
        format_public_key_fingerprint, host_key_id, known_host_entry_from_key, verify_host_key,
        HostKeyDecision, KnownHostEntry,
    };
    use russh::keys::ssh_key::PublicKey;
    use std::collections::HashMap;
    use std::str::FromStr;

    const SAMPLE_ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti";

    fn sample_ed25519_key() -> PublicKey {
        PublicKey::from_openssh(SAMPLE_ED25519).expect("sample ed25519 key")
    }

    fn sample_ed25519_entry() -> KnownHostEntry {
        known_host_entry_from_key(&sample_ed25519_key()).expect("sample ed25519 entry")
    }

    #[test]
    fn host_key_id_formats_host_port() {
        assert_eq!(host_key_id("jump.example.com", 22), "jump.example.com:22");
        assert_eq!(host_key_id("127.0.0.1", 2222), "127.0.0.1:2222");
    }

    #[test]
    fn supported_auth_includes_agent() {
        assert!(supported_auth_method("password"));
        assert!(supported_auth_method("private_key"));
        assert!(supported_auth_method("agent"));
        assert!(!supported_auth_method("keyboard"));
    }

    #[test]
    fn fingerprint_is_stable_sha256_openssh_format() {
        let key = sample_ed25519_key();
        let fp = format_public_key_fingerprint(&key);
        assert!(fp.starts_with("SHA256:"));
        assert_eq!(fp, sample_ed25519_entry().fingerprint);
    }

    #[test]
    fn verify_first_seen_accepts_and_reports_fingerprint() {
        let observed = sample_ed25519_entry();
        match verify_host_key(None, &observed) {
            HostKeyDecision::AcceptFirstUse { fingerprint } => {
                assert_eq!(fingerprint, observed.fingerprint);
            }
            other => panic!("expected AcceptFirstUse, got {other:?}"),
        }
    }

    #[test]
    fn verify_matching_key_accepts() {
        let stored = sample_ed25519_entry();
        let observed = sample_ed25519_entry();
        assert_eq!(
            verify_host_key(Some(&stored), &observed),
            HostKeyDecision::AcceptMatch
        );
    }

    #[test]
    fn verify_changed_key_rejects_with_fingerprints() {
        let stored = sample_ed25519_entry();
        let mut other = stored.clone();
        other.fingerprint = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into();

        match verify_host_key(Some(&stored), &other) {
            HostKeyDecision::RejectMismatch { expected, received } => {
                assert_eq!(expected, stored.fingerprint);
                assert_eq!(received, other.fingerprint);
            }
            other => panic!("expected RejectMismatch, got {other:?}"),
        }
    }

    #[test]
    fn mismatch_error_message_includes_host_and_fingerprints() {
        let msg =
            mismatch_error_message("evil.example.com:22", "SHA256:expected", "SHA256:received");
        assert!(msg.contains("evil.example.com:22"));
        assert!(msg.contains("SHA256:expected"));
        assert!(msg.contains("SHA256:received"));
        assert!(msg.contains("MITM"));
    }

    #[test]
    fn known_hosts_roundtrip_via_tempfile() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ssh_known_hosts.json");

        assert!(load_known_hosts(&path).is_empty());

        let entry = sample_ed25519_entry();
        let mut map = HashMap::new();
        map.insert(host_key_id("host.example", 22), entry.clone());
        save_known_hosts(&path, &map).expect("save known hosts");

        let loaded = load_known_hosts(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("host.example:22"), Some(&entry));
    }

    #[test]
    fn load_known_hosts_ignores_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing.json");
        assert!(load_known_hosts(&path).is_empty());
    }

    #[test]
    fn load_known_hosts_ignores_invalid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ssh_known_hosts.json");
        std::fs::write(&path, "{ not json").expect("write invalid json");
        assert!(load_known_hosts(&path).is_empty());
    }

    #[test]
    fn known_host_entry_captures_algorithm_and_openssh_blob() {
        let entry = sample_ed25519_entry();
        assert_eq!(entry.algorithm, "ssh-ed25519");
        assert!(entry.public_key.starts_with("ssh-ed25519 "));
        assert!(PublicKey::from_str(&entry.public_key).is_ok());
    }

    #[test]
    fn expand_home_expands_tilde_prefix() {
        assert_eq!(expand_home("/absolute/path"), "/absolute/path");
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            assert_eq!(expand_home("~/keys/id_rsa"), format!("{home}/keys/id_rsa"));
        }
    }
}
