//! SSH tunnel for forwarding database connections through an SSH jump host.
//!
//! When `SshTunnelConfig.enabled` is true, we:
//! 1. Open an SSH session to the jump host
//! 2. Authenticate (password or private key)
//! 3. Bind a local TCP listener on 127.0.0.1:<random>
//! 4. For every incoming local connection, open a `direct-tcpip` channel
//!    through the SSH session and pipe data bidirectionally.
//! 5. Return the local port so DB drivers can connect to 127.0.0.1:<local>.

use super::domain::KnownHostsRepository;
use super::ssh_known_hosts::{
    host_key_id, known_host_entry_from_key, mismatch_error_message, verify_host_key,
    HostKeyDecision, KnownHostEntry,
};
use datazen_driver_api::{DriverError, SshTunnelConfig};
use russh::client::{self, AuthResult};
use russh::keys::{self, ssh_key, PrivateKeyWithHashAlg};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

// ── SSH client handler (TOFU host key verification) ─────────────────

struct TunnelHandler {
    host_id: String,
    /// 已知主机的**存储**：宿主注入的端口实例（桌面：文件；web：服务端存储）。
    known_hosts: Arc<dyn KnownHostsRepository>,
    /// 本次会话已知的条目。首见即写入存储，因此内存里始终是存储的镜像。
    entries: Arc<Mutex<HashMap<String, KnownHostEntry>>>,
    rejection: Arc<Mutex<Option<String>>>,
}

impl client::Handler for TunnelHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        key: &ssh_key::PublicKey,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let host_id = self.host_id.clone();
        let known_hosts = self.known_hosts.clone();
        let entries = self.entries.clone();
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

            let mut map = match entries.lock() {
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
                    if let Err(e) = known_hosts.save(&map) {
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
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    _upstream: Option<Box<SshTunnel>>,
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        // Dropping a `JoinHandle` only *detaches* the task (tokio semantics), so
        // without this the forwarding loop, the bound 127.0.0.1 port and the SSH
        // session (kept alive by the task's `Arc<Mutex<Handle>>` clone) would
        // leak for the lifetime of the process. Cancelling first lets in-flight
        // per-connection forwarders unwind, then `abort` stops the accept loop;
        // once the last handle clone is gone the russh session task ends and the
        // SSH connection closes. `_upstream` (jump-host chain) drops after this
        // body and tears itself down the same way.
        self.cancel.cancel();
        self.task.abort();
    }
}

pub fn supported_auth_method(method: &str) -> bool {
    matches!(method, "password" | "private_key" | "agent")
}

impl SshTunnel {
    /// Establish an SSH tunnel that forwards `127.0.0.1:<local_port>` →
    /// `remote_host:remote_port` through the configured SSH jump host.
    ///
    /// `known_hosts` 是宿主注入的**存储端口**实例：已知主机存在哪儿由实现方
    /// 决定（桌面：`{appData}/ssh_known_hosts.json`；web：服务端存储）。
    pub async fn start(
        ssh: &SshTunnelConfig,
        remote_host: &str,
        remote_port: u16,
        known_hosts: Arc<dyn KnownHostsRepository>,
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
                    known_hosts.clone(),
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
        let entries = Arc::new(Mutex::new(known_hosts.load()));

        let handler = TunnelHandler {
            host_id: host_id.clone(),
            known_hosts,
            entries,
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
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut tcp_stream, _) = tokio::select! {
                    _ = task_cancel.cancelled() => break,
                    accept = listener.accept() => match accept {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("SSH tunnel accept error: {e}");
                            break;
                        }
                    },
                };

                let rh = rh.clone();
                let session = session.clone();
                let child_cancel = task_cancel.clone();
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
                    tokio::select! {
                        _ = child_cancel.cancelled() => {}
                        _ = tokio::io::copy_bidirectional(&mut tcp_stream, &mut ssh_stream) => {}
                    }
                });
            }
        });

        Ok(SshTunnel {
            local_port,
            cancel,
            task,
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
    use super::super::ssh_known_hosts::{
        format_public_key_fingerprint, host_key_id, known_host_entry_from_key, verify_host_key,
        HostKeyDecision, KnownHostEntry,
    };
    use super::*;
    use russh::keys::ssh_key::PublicKey;
    use std::collections::HashMap;
    use std::str::FromStr;

    const SAMPLE_ED25519: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti";

    /// 形态中立的进程内 known-hosts 存储：隧道机制的单测**不碰本机文件**
    /// （文件 I/O 是宿主实现的事，见 `KnownHostsRepository`）。
    #[derive(Default)]
    struct MemoryKnownHosts {
        entries: std::sync::Mutex<HashMap<String, KnownHostEntry>>,
    }

    impl KnownHostsRepository for MemoryKnownHosts {
        fn load(&self) -> HashMap<String, KnownHostEntry> {
            self.entries
                .lock()
                .map(|entries| entries.clone())
                .unwrap_or_default()
        }

        fn save(
            &self,
            entries: &HashMap<String, KnownHostEntry>,
        ) -> Result<(), crate::shared::error::PortError> {
            let mut guard = self.entries.lock().map_err(|_| {
                crate::shared::error::PortError::BackendUnavailable(
                    "known hosts lock poisoned".into(),
                )
            })?;
            *guard = entries.clone();
            Ok(())
        }
    }

    fn memory_known_hosts() -> Arc<dyn KnownHostsRepository> {
        Arc::new(MemoryKnownHosts::default())
    }

    /// Regression for `tunnel-backend-BUG-002`: `SshTunnel` used to hold only a
    /// `JoinHandle`, and dropping a `JoinHandle` *detaches* rather than aborts,
    /// so every probe leaked the forwarder task, the SSH session and the bound
    /// 127.0.0.1 port.
    ///
    /// No SSH endpoint exists in unit tests, so this builds the minimal shape
    /// `start` produces (a bound listener owned by a spawned forwarder) and
    /// proves that `drop` cancels + aborts it and that the port is released.
    #[tokio::test]
    async fn drop_cancels_and_aborts_the_forwarder_and_releases_the_local_port() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture listener");
        let local_port = listener.local_addr().expect("fixture addr").port();

        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::select! {
                _ = task_cancel.cancelled() => {}
                _ = listener.accept() => {}
            }
            // `listener` is dropped here, which is what frees the port.
        });
        let abort = task.abort_handle();

        let observed = cancel.clone();
        let tunnel = SshTunnel {
            local_port,
            cancel,
            task,
            _upstream: None,
        };
        assert_eq!(tunnel.local_port(), local_port);
        assert!(!observed.is_cancelled(), "fixture must start uncancelled");

        drop(tunnel);

        assert!(
            observed.is_cancelled(),
            "SshTunnel::drop must cancel the forwarder (BUG-002 regression)"
        );

        let mut aborted = false;
        for _ in 0..200 {
            tokio::task::yield_now().await;
            if abort.is_finished() {
                aborted = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            aborted,
            "SshTunnel::drop must abort the forwarder task (BUG-002 regression)"
        );

        // A detach-only drop would keep 127.0.0.1:<local_port> bound forever.
        let rebound = TcpListener::bind(("127.0.0.1", local_port)).await;
        assert!(
            rebound.is_ok(),
            "SshTunnel::drop must release 127.0.0.1:{local_port}: {:?}",
            rebound.err()
        );
    }

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
    fn known_hosts_roundtrip_goes_through_the_store_port() {
        // 文件 I/O 不再住在共享包里：往返由**端口**保证，宿主实现（桌面：
        // `{appData}/ssh_known_hosts.json`）另有自己的往返单测。
        let store = memory_known_hosts();
        assert!(store.load().is_empty());

        let entry = sample_ed25519_entry();
        let mut map = HashMap::new();
        map.insert(host_key_id("host.example", 22), entry.clone());
        store.save(&map).expect("save known hosts");

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("host.example:22"), Some(&entry));
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

    // ── [tester round 2] real-path verification against the in-process bastion ──

    /// [tester] BUG-002 on the **real** `SshTunnel::start` path (the coder's
    /// regression test constructs a synthetic struct instead). Against a live
    /// bastion it proves: (1) bytes really flow through `direct-tcpip`;
    /// (2) `drop` finishes the real forwarding task; (3) the forwarded
    /// connection is torn down; (4) the real local port is bindable again;
    /// (5) the SSH session itself closes once the last
    /// `Arc<Mutex<client::Handle>>` clone is gone.
    #[tokio::test]
    async fn test_tester_ssh_drop_aborts_the_real_forwarder_and_closes_the_session() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let bastion = test_fixture::spawn_bastion().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let key_path = test_fixture::write_client_key(dir.path());
        let known_hosts = memory_known_hosts();
        let (echo_port, echo_task) = test_fixture::spawn_echo_target().await;

        let ssh = test_fixture::ssh_config(bastion.port, &key_path);
        let tunnel = SshTunnel::start(&ssh, "127.0.0.1", echo_port, known_hosts.clone())
            .await
            .expect("the fixture bastion must accept the tunnel");
        let local_port = tunnel.local_port();
        let abort = tunnel.task.abort_handle();

        let mut client = TcpStream::connect(("127.0.0.1", local_port))
            .await
            .expect("connect the local forwarder");
        client.write_all(b"ping").await.expect("write ping");
        let mut echoed = [0u8; 4];
        tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed))
            .await
            .expect("forwarding timed out")
            .expect("forwarded read");
        assert_eq!(&echoed, b"ping", "real bytes must traverse the bastion");
        assert_eq!(
            bastion.channel_opens.load(Ordering::SeqCst),
            1,
            "exactly one direct-tcpip channel must have been opened"
        );
        assert_eq!(
            bastion.live_sessions.load(Ordering::SeqCst),
            1,
            "exactly one SSH session must be live before drop"
        );

        drop(tunnel);

        let mut finished = false;
        for _ in 0..200 {
            if abort.is_finished() {
                finished = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            finished,
            "drop must abort the real SSH forwarder task (BUG-002)"
        );

        let mut tail = [0u8; 1];
        match tokio::time::timeout(Duration::from_secs(3), client.read(&mut tail)).await {
            Ok(Ok(0)) | Ok(Err(_)) => {}
            other => panic!("the forwarded connection must close on drop, got {other:?}"),
        }
        drop(client);

        let mut rebound = false;
        for _ in 0..100 {
            if TcpListener::bind(("127.0.0.1", local_port)).await.is_ok() {
                rebound = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            rebound,
            "drop must release 127.0.0.1:{local_port} (detach-only drop keeps it bound)"
        );

        let mut closed = false;
        for _ in 0..300 {
            if bastion.live_sessions.load(Ordering::SeqCst) == 0 {
                closed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            closed,
            "drop must close the SSH session (still live: {})",
            bastion.live_sessions.load(Ordering::SeqCst)
        );

        echo_task.abort();
        bastion.shutdown();
    }

    /// [tester] (a)5 — semantic boundary of the SSH probe, proven with the
    /// in-process bastion: `SshTunnel::start` dials and authenticates but never
    /// opens a `direct-tcpip` channel, so a tunnel whose *target* is
    /// unreachable still reports success. Only the first real forwarded
    /// connection discovers the dead target.
    #[tokio::test]
    async fn test_tester_ssh_probe_does_not_prove_target_reachability() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let bastion = test_fixture::spawn_bastion().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let key_path = test_fixture::write_client_key(dir.path());
        let known_hosts = memory_known_hosts();
        let ssh = test_fixture::ssh_config(bastion.port, &key_path);

        // Port 1 on the bastion host has no listener: the fixture refuses the
        // `direct-tcpip` channel for it, exactly like a real bastion would.
        let tunnel = SshTunnel::start(&ssh, "127.0.0.1", 1, known_hosts.clone())
            .await
            .expect("bastion reachability + auth is all the SSH probe proves");
        assert_eq!(
            bastion.channel_opens.load(Ordering::SeqCst),
            0,
            "start must not open a channel for the target"
        );

        let mut client = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .expect("connect the local forwarder");
        let _ = client.write_all(b"ping").await;
        let mut buf = [0u8; 4];
        match tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => {}
            other => panic!(
                "an unreachable target must fail the first forwarded connection, got {other:?}"
            ),
        }
        assert_eq!(
            bastion.channel_opens.load(Ordering::SeqCst),
            1,
            "the real forwarded connection is what opens the channel"
        );
        drop(tunnel);
        bastion.shutdown();
    }

    /// [tester] (b)3 — the jump-host chain (`_upstream`) must be torn down
    /// recursively by the field drop. Two in-process bastions: the outer tunnel
    /// reaches the second bastion *through* the first one.
    #[tokio::test]
    async fn test_tester_ssh_jump_chain_drop_tears_down_every_bastion() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let jump_bastion = test_fixture::spawn_bastion().await;
        let target_bastion = test_fixture::spawn_bastion().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let key_path = test_fixture::write_client_key(dir.path());
        let known_hosts = memory_known_hosts();
        let (echo_port, echo_task) = test_fixture::spawn_echo_target().await;

        let mut ssh = test_fixture::ssh_config(target_bastion.port, &key_path);
        ssh.jump = Some(Box::new(test_fixture::ssh_config(
            jump_bastion.port,
            &key_path,
        )));

        let tunnel = SshTunnel::start(&ssh, "127.0.0.1", echo_port, known_hosts.clone())
            .await
            .expect("the chained tunnel must build through both bastions");
        assert_eq!(jump_bastion.live_sessions.load(Ordering::SeqCst), 1);
        assert_eq!(target_bastion.live_sessions.load(Ordering::SeqCst), 1);

        let mut client = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .expect("connect the local forwarder");
        client.write_all(b"ping").await.expect("write ping");
        let mut echoed = [0u8; 4];
        tokio::time::timeout(Duration::from_secs(5), client.read_exact(&mut echoed))
            .await
            .expect("chained forwarding timed out")
            .expect("chained read");
        assert_eq!(&echoed, b"ping");

        drop(tunnel);

        for (name, bastion) in [("jump", &jump_bastion), ("target", &target_bastion)] {
            let mut closed = false;
            for _ in 0..300 {
                if bastion.live_sessions.load(Ordering::SeqCst) == 0 {
                    closed = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                closed,
                "drop must tear the {name} bastion session down (still live: {})",
                bastion.live_sessions.load(Ordering::SeqCst)
            );
        }
        echo_task.abort();
        jump_bastion.shutdown();
        target_bastion.shutdown();
    }
}

/// [tester round 2] In-process SSH bastion fixture.
///
/// The tunnel tests need a *live* SSH endpoint to exercise the real
/// `SshTunnel::start` path (the coder's BUG-002 regression test builds a
/// synthetic struct instead). macOS `sshd` cannot run unprivileged here
/// (`ssh_sandbox_child: sandbox_init: Operation not permitted`), so this
/// fixture speaks SSH with `russh`'s server API in-process: it accepts any
/// public key, forwards `direct-tcpip` channels to the requested
/// `host:port`, and exposes counters for live sessions and opened channels.
#[cfg(test)]
pub(crate) mod test_fixture {
    use datazen_driver_api::SshTunnelConfig;
    use russh::server::{Auth, Config, Msg, Server as _, Session};
    use russh::Channel;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    /// Throwaway ed25519 key pair generated for tests only (`ssh-keygen -t
    /// ed25519 -N '' -C datazen-test-fixture`); it protects nothing.
    pub(crate) const FIXTURE_SSH_PRIVATE_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDT+r9pHdMvWoPiPcU0cMrUpYpGivMUJCZ7mqhnR+A0VAAAAJhVh/HoVYfx
6AAAAAtzc2gtZWQyNTUxOQAAACDT+r9pHdMvWoPiPcU0cMrUpYpGivMUJCZ7mqhnR+A0VA
AAAEC8M9yMoCle2oAaYYXKDQ7nYjJCccXBVwo9ZvFbCzZ9mNP6v2kd0y9ag+I9xTRwytSl
ikaK8xQkJnuaqGdH4DRUAAAAFGRhdGF6ZW4tdGVzdC1maXh0dXJlAQ==
-----END OPENSSH PRIVATE KEY-----
";

    /// A running in-process bastion.
    pub(crate) struct Bastion {
        pub(crate) port: u16,
        /// SSH sessions the bastion currently has open. Incremented per
        /// accepted TCP connection, decremented when the session handler drops.
        pub(crate) live_sessions: Arc<AtomicUsize>,
        /// `direct-tcpip` channels the client actually asked for.
        pub(crate) channel_opens: Arc<AtomicUsize>,
        task: tokio::task::JoinHandle<()>,
    }

    impl Bastion {
        pub(crate) fn shutdown(&self) {
            self.task.abort();
        }
    }

    #[derive(Clone)]
    struct FixtureBastion {
        live_sessions: Arc<AtomicUsize>,
        channel_opens: Arc<AtomicUsize>,
    }

    struct FixtureHandler {
        live_sessions: Arc<AtomicUsize>,
        channel_opens: Arc<AtomicUsize>,
    }

    impl Drop for FixtureHandler {
        fn drop(&mut self) {
            self.live_sessions.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl russh::server::Server for FixtureBastion {
        type Handler = FixtureHandler;

        fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> FixtureHandler {
            self.live_sessions.fetch_add(1, Ordering::SeqCst);
            FixtureHandler {
                live_sessions: self.live_sessions.clone(),
                channel_opens: self.channel_opens.clone(),
            }
        }
    }

    impl russh::server::Handler for FixtureHandler {
        type Error = russh::Error;

        async fn auth_publickey(
            &mut self,
            _user: &str,
            _public_key: &russh::keys::ssh_key::PublicKey,
        ) -> Result<Auth, Self::Error> {
            Ok(Auth::Accept)
        }

        async fn channel_open_direct_tcpip(
            &mut self,
            channel: Channel<Msg>,
            host_to_connect: &str,
            port_to_connect: u32,
            _originator_address: &str,
            _originator_port: u32,
            _session: &mut Session,
        ) -> Result<bool, Self::Error> {
            self.channel_opens.fetch_add(1, Ordering::SeqCst);
            // Connect before acknowledging so an unreachable target produces a
            // channel-open failure, exactly like a real bastion.
            let tcp =
                match tokio::net::TcpStream::connect((host_to_connect, port_to_connect as u16))
                    .await
                {
                    Ok(tcp) => tcp,
                    Err(_) => return Ok(false),
                };
            tokio::spawn(async move {
                let mut tcp = tcp;
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
            });
            Ok(true)
        }
    }

    /// Bind a loopback bastion and serve SSH on it.
    pub(crate) async fn spawn_bastion() -> Bastion {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture bastion");
        let port = listener.local_addr().expect("bastion addr").port();
        let live_sessions = Arc::new(AtomicUsize::new(0));
        let channel_opens = Arc::new(AtomicUsize::new(0));

        let live = live_sessions.clone();
        let opens = channel_opens.clone();
        let task = tokio::spawn(async move {
            let key = russh::keys::PrivateKey::from_openssh(FIXTURE_SSH_PRIVATE_KEY)
                .expect("fixture host key must parse");
            let config = Arc::new(Config {
                keys: vec![key],
                inactivity_timeout: Some(std::time::Duration::from_secs(60)),
                auth_rejection_time: std::time::Duration::from_millis(0),
                auth_rejection_time_initial: Some(std::time::Duration::from_millis(0)),
                ..Default::default()
            });
            let mut bastion = FixtureBastion {
                live_sessions: live,
                channel_opens: opens,
            };
            let server = bastion.run_on_socket(config, &listener);
            let _ = server.await;
        });

        Bastion {
            port,
            live_sessions,
            channel_opens,
            task,
        }
    }

    /// Write the throwaway client key into `dir` and return its path.
    pub(crate) fn write_client_key(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("datazen_test_ed25519");
        std::fs::write(&path, FIXTURE_SSH_PRIVATE_KEY).expect("write fixture client key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        path
    }

    /// `SshTunnelConfig` pointing at the fixture bastion with key auth.
    pub(crate) fn ssh_config(port: u16, key_path: &Path) -> SshTunnelConfig {
        SshTunnelConfig {
            enabled: true,
            host: "127.0.0.1".into(),
            port,
            username: "tester".into(),
            auth_method: "private_key".into(),
            password: None,
            private_key_path: Some(key_path.to_string_lossy().into_owned()),
            passphrase: None,
            jump: None,
        }
    }

    /// A loopback echo server; returns its port and a task handle.
    pub(crate) async fn spawn_echo_target() -> (u16, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind echo target");
        let port = listener.local_addr().expect("echo addr").port();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            }
        });
        (port, task)
    }
}
