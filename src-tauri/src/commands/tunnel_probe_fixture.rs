//! Host-side copy of the in-process SSH bastion fixture.
//!
//! ## Why this file exists at all
//!
//! The fixture itself is test-support code for the SSH tunnel: an in-process
//! SSH server (via `russh`'s server API) that accepts any public key, forwards
//! `direct-tcpip` channels to the requested `host:port`, and counts live
//! sessions and opened channels. It exists because macOS `sshd` cannot run
//! unprivileged here (`ssh_sandbox_child: sandbox_init: Operation not
//! permitted`), so the tunnel tests need a *live* SSH endpoint to exercise the
//! real `SshTunnel::start` path.
//!
//! The tunnel mechanisms now live in the shared crate `datazen-tunnel`, which
//! keeps its own copy of the fixture for its own tests
//! (`packages/tunnel/src/ssh.rs`, gated on `#[cfg(test)]`). This file is the
//! host's copy, because the host's test suite drives the fixture across the
//! crate boundary and Cargo cannot hand the host's lib-test target a
//! differently-featured build of a crate the host also depends on normally:
//!
//!   * a `[dev-dependencies]` entry with `features = ["test-harness"]` is not
//!     unified into the normal dependency graph (`resolver = "2"`), so the
//!     lib-test target links the build *without* the feature — the fixture is
//!     simply not there;
//!   * a `[target.'cfg(test)'.dependencies]` entry is not activated for the
//!     lib-test target either.
//!
//! So the fixture has to be reachable from the host's own test build, and the
//! only way to get it there without putting it into a production artifact is a
//! host-side copy.
//!
//! ## Why that does not put a private key into the shipped binary
//!
//! `FIXTURE_SSH_PRIVATE_KEY` is a throwaway ed25519 key pair that is already
//! committed to this repository and protects nothing (it is the key of a
//! loopback-only server that exists for the lifetime of a test). Keeping this
//! copy behind `#[cfg(test)]` keeps it out of every production artifact.
//!
//! That is not a convention here, it is a test: the
//! `ssh_bastion_fixture_never_reaches_a_production_build` case at the bottom of
//! this file scans `src-tauri/src/**` for the constant name and fails if it
//! shows up anywhere except in this file, and separately checks that this
//! file's module declaration is `#[cfg(test)]`-gated.

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
            match tokio::net::TcpStream::connect((host_to_connect, port_to_connect as u16)).await {
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
///
/// Kept for parity with the shared crate's copy of this fixture, so the two
/// stay comparable line by line; the host's own test builds its tunnel config
/// inline instead.
#[allow(dead_code)]
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
///
/// Kept for parity with the shared crate's copy of this fixture, so the two
/// stay comparable line by line; the host's own test does not need an echo
/// target.
#[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    /// The host's own production source tree must not contain the fixture.
    ///
    /// The fixture is test-support code, and the reason it is duplicated
    /// rather than shared is precisely that a shared copy would have to be
    /// compiled into the shipped binary (see the module doc). That reasoning
    /// only holds as long as the copy really is test-only, so the fact is
    /// checked here instead of being left to convention.
    #[test]
    fn ssh_bastion_fixture_never_reaches_a_production_build() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir
            .parent()
            .expect("src-tauri sits in the repo root");
        let host_src = manifest_dir.join("src");

        let mut files = Vec::new();
        collect_rust_files(&host_src, &mut files);

        let mut holders: Vec<String> = files
            .iter()
            .filter(|file| {
                std::fs::read_to_string(file)
                    .map(|text| text.contains("FIXTURE_SSH_PRIVATE_KEY"))
                    .unwrap_or(false)
            })
            .map(|file| {
                file.strip_prefix(&host_src)
                    .expect("every file is under src-tauri/src")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        holders.sort();

        assert_eq!(
            holders,
            vec!["commands/tunnel_probe_fixture.rs".to_string()],
            "the SSH bastion fixture (and the throwaway private key it carries) must live in \
             exactly one file of the host's source tree, and that file must be the test-only one"
        );

        // The file holding it must be pulled in by a `#[cfg(test)]`-gated
        // module declaration, otherwise it is part of the production lib.
        let commands_mod = std::fs::read_to_string(host_src.join("commands/mod.rs"))
            .expect("read commands/mod.rs");
        let decl = commands_mod
            .find("mod tunnel_probe_fixture;")
            .expect("the fixture module must be declared in commands/mod.rs");
        assert!(
            commands_mod[decl.saturating_sub(200)..decl].contains("#[cfg(test)]"),
            "the fixture module must be declared under #[cfg(test)] so it stays out of the \
             production build"
        );

        // The shared crate keeps its own copy for its own tests, and that copy
        // must stay behind `#[cfg(test)]` as well — no host-opened feature may
        // pull it into a production artifact.
        let tunnel_ssh = std::fs::read_to_string(repo_root.join("packages/tunnel/src/ssh.rs"))
            .expect("read packages/tunnel/src/ssh.rs");
        let fixture_mod = tunnel_ssh
            .find("mod test_fixture {")
            .expect("the shared crate keeps its own fixture module");
        let window = &tunnel_ssh[fixture_mod.saturating_sub(200)..fixture_mod];
        assert!(
            window.contains("#[cfg(test)]"),
            "the shared crate's fixture module must be gated on #[cfg(test)]"
        );
        assert!(
            !window.contains("test-harness"),
            "the shared crate's fixture module must not be gated on a feature the host turns on, \
             or it would be compiled into the shipped binary"
        );
    }

    fn collect_rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
}
