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
use russh::client::{self, AuthResult};
use russh::keys::{self, PrivateKeyWithHashAlg, ssh_key};
use std::future::Future;
use std::sync::Arc;
use tokio::net::TcpListener;

// ── SSH client handler (accept all host keys) ───────────────────────

struct TunnelHandler;

impl client::Handler for TunnelHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        _key: &ssh_key::PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        async { Ok(true) }
    }
}

// ── Public tunnel struct ────────────────────────────────────────────

pub struct SshTunnel {
    local_port: u16,
    _task: tokio::task::JoinHandle<()>,
}

impl SshTunnel {
    /// Establish an SSH tunnel that forwards `127.0.0.1:<local_port>` →
    /// `remote_host:remote_port` through the configured SSH jump host.
    pub async fn start(
        ssh: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<Self, DriverError> {
        let config = Arc::new(client::Config::default());

        // 1. Connect
        let mut session = client::connect(
            config,
            (ssh.host.as_str(), ssh.port),
            TunnelHandler,
        )
        .await
        .map_err(|e| DriverError::SshTunnelError(format!("SSH connect to {}:{} failed: {e}", ssh.host, ssh.port)))?;

        // 2. Authenticate
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
                let key_path = ssh
                    .private_key_path
                    .as_deref()
                    .unwrap_or("~/.ssh/id_rsa");
                let expanded = expand_home(key_path);

                let secret_key = keys::load_secret_key(&expanded, ssh.passphrase.as_deref())
                    .map_err(|e| DriverError::SshTunnelError(format!("Load SSH key {expanded}: {e}")))?;

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
            other => {
                return Err(DriverError::SshTunnelError(format!(
                    "Unknown SSH auth method: {other}"
                )));
            }
        }

        // 3. Bind local listener
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| DriverError::SshTunnelError(format!("Bind local port: {e}")))?;
        let local_port = listener.local_addr().unwrap().port();

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
                            .channel_open_direct_tcpip(rh, remote_port as u32, "127.0.0.1", lp as u32)
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
        })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
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
