//! A private `redis-server` for this crate's live tests.
//!
//! Measured on the machine this was written: `redis-server` is installed;
//! `docker`, `podman` and `colima` are all absent; and **something is listening
//! on 6379**.
//!
//! * **A container is not an option here.** No container runtime is installed,
//!   and adding one as a prerequisite would make the suite unrunnable in the
//!   environment it was written in.
//! * **Reusing whatever is on 6379 would be worse than not testing at all.** It
//!   would write keys into whatever database a developer has open, and it would
//!   make the suite's outcome depend on their data. That is the same defect
//!   that was already fixed once in this crate: an `acquire_resource` aimed at
//!   6379 *succeeded* against an unrelated server, and a test written to
//!   require an error passed for the wrong reason.
//! * **A fake RESP server proves the wrong thing.** It would show that the
//!   client can talk to something that answers like Redis. What is claimed is
//!   that `SELECT <n>` moves *this* connection onto *that* logical database on
//!   *that* server, and only the server can settle that.
//!
//! So the harness starts a private `redis-server` child: a port it probed
//! itself with `bind(127.0.0.1:0)` rather than a constant, its own data
//! directory under the system temp dir, and persistence switched off
//! (`--save "" --appendonly no`) so the child cannot write a `dump.rdb` next to
//! a developer's data or into this repository. The child is killed and the
//! directory removed in [`LiveRedis`]'s `Drop`. Nothing outside the child
//! process is contacted.
//!
//! ## When it skips, and when it does not
//!
//! `redis-server` not being on `PATH` is the **only** skip condition:
//! [`LiveRedis::start`] returns `Ok(None)` and the caller prints why. A build
//! machine without Redis installed must stay green. Everything else fails
//! loudly. A binary that starts but never answers, a port that never becomes
//! reachable, a data directory that cannot be created — each of those is a
//! broken harness, and treating a broken harness as "no dependency available"
//! is exactly how a live suite quietly stops being live.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

/// How long the child gets to answer `PING` before the harness calls it
/// broken. Redis with persistence disabled starts in milliseconds; ten seconds
/// is a ceiling for a loaded machine, not an expectation.
pub(crate) const STARTUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Distinguishes concurrent tests' data directories within one process.
static SERVER_SEQ: AtomicU32 = AtomicU32::new(0);

/// Overrides the server binary. Set `DATAZEN_REDIS_SERVER=/path/to/redis-server`
/// when the binary is installed but not on `PATH`.
pub(crate) const BINARY_ENV: &str = "DATAZEN_REDIS_SERVER";

/// A private `redis-server` child, killed and cleaned up on drop.
///
/// Nothing here mutates after construction; the only mutation is killing the
/// child, which happens in `Drop`.
pub(crate) struct LiveRedis {
    child: Child,
    port: u16,
    data_dir: PathBuf,
}

impl LiveRedis {
    /// Start a server on a port and a data directory of this test's own.
    ///
    /// `Ok(None)` means only one thing: the binary is not installed. Every other
    /// failure — including one from the server process itself — is an `Err`, so
    /// a broken harness is reported instead of being mistaken for a missing
    /// dependency.
    pub(crate) fn start() -> Result<Option<Self>, String> {
        let binary = std::env::var(BINARY_ENV).unwrap_or_else(|_| "redis-server".to_string());
        let port = probe_port()?;
        let data_dir = fresh_data_dir()?;

        let child = Command::new(&binary)
            .arg("--port")
            .arg(port.to_string())
            .arg("--bind")
            .arg("127.0.0.1")
            .arg("--dir")
            .arg(&data_dir)
            // No persistence: the child must never write a `dump.rdb` next to a
            // developer's real data, and a shutdown save would be worse.
            .arg("--save")
            .arg("")
            .arg("--appendonly")
            .arg("no")
            .arg("--daemonize")
            .arg("no")
            .arg("--loglevel")
            .arg("warning")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        let child = match child {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = std::fs::remove_dir_all(&data_dir);
                return Ok(None);
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&data_dir);
                return Err(format!("starting `{binary}` failed: {e}"));
            }
        };

        Ok(Some(Self {
            child,
            port,
            data_dir,
        }))
    }

    /// Wait until the child actually answers `PING`.
    ///
    /// A successful TCP connect is not readiness — Redis binds the listener
    /// before it is ready to serve, and racing it would make every assertion
    /// that uses this flaky. So the gate is a real command.
    pub(crate) async fn wait_until_serving(&self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;
        let url = self.url();
        loop {
            if let Ok(client) = redis::Client::open(url.as_str()) {
                if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                    if let Ok(reply) = redis::cmd("PING").query_async::<String>(&mut conn).await {
                        if reply == "PONG" {
                            return Ok(());
                        }
                    }
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!(
                    "the redis-server child on port {} never answered PING within \
                     {STARTUP_TIMEOUT:?}. This is a harness failure, not a missing dependency: \
                     the binary ran but the server did not become usable.",
                    self.port
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    /// The port this child was told to bind.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// `redis://127.0.0.1:<port>` with no database in the path, so a client
    /// lands on db 0 and every `SELECT` is explicit.
    pub(crate) fn url(&self) -> String {
        format!("redis://127.0.0.1:{}", self.port)
    }
}

impl Drop for LiveRedis {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

/// Ask the OS for a free port rather than naming one.
///
/// The listener is dropped before `redis-server` binds, so a competing process
/// could in principle take the port in between. That is why
/// [`LiveRedis::wait_until_serving`] checks with a real `PING` instead of
/// assuming the bind succeeded: the failure mode is a loud `Err` naming the
/// port, never a silently skipped test.
fn probe_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("could not reserve an ephemeral port for the live server: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("could not read the reserved port: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn fresh_data_dir() -> Result<PathBuf, String> {
    let n = SERVER_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("datazen-redis-live-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create the live server's data dir {dir:?}: {e}"))?;
    Ok(dir)
}

/// Start a server, or explain that Redis is not installed.
///
/// Every live test funnels through this so the skip reason is printed once, in
/// one place, instead of being restated (and drifting) per test.
macro_rules! live_server_or_skip {
    () => {
        match crate::live_server::LiveRedis::start() {
            Ok(Some(server)) => server,
            Ok(None) => {
                eprintln!(
                    "SKIPPED: no `redis-server` on PATH. Set {} to point at one. The live suite \
                     needs a real Redis to have anything to prove.",
                    crate::live_server::BINARY_ENV
                );
                return;
            }
            Err(reason) => panic!("{reason}"),
        }
    };
}

pub(crate) use live_server_or_skip;
