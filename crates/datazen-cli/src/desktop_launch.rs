//! Cross-platform launch of the DataZen desktop host.
//!
//! The CLI never invents a deep link. It starts a real desktop binary with an
//! argv contract the desktop parses at startup:
//!
//! `--datazen-open-connection <id> [--datazen-open-query <sql>]`
//! `[--datazen-data-dir <path>] [--datazen-open-ack <path>]`
//!
//! The launch is acknowledged only after the desktop process writes the ack
//! file. A reused/failed/forgotten instance therefore cannot make the CLI emit
//! a false `launched: true`.
//!
//! `DATAZEN_DESKTOP_BIN` is an explicit test/advanced override; normal installs
//! use the platform application locations below.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{CliError, E_EXECUTOR, E_INTERNAL, E_NOT_FOUND, E_TIMEOUT};

const OPEN_CONNECTION_ARG: &str = "--datazen-open-connection";
const OPEN_QUERY_ARG: &str = "--datazen-open-query";
const DATA_DIR_ARG: &str = "--datazen-data-dir";
const OPEN_ACK_ARG: &str = "--datazen-open-ack";
const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_secs(5);
const ACK_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub fn launch(
    connection_id: &str,
    query: Option<&str>,
    data_dir: Option<&Path>,
) -> Result<(), CliError> {
    let ack = AckFile::create()?;
    let mut forwarded = vec![OPEN_CONNECTION_ARG.to_string(), connection_id.to_string()];
    if let Some(query) = query {
        forwarded.push(OPEN_QUERY_ARG.to_string());
        forwarded.push(query.to_string());
    }
    if let Some(data_dir) = data_dir {
        forwarded.push(DATA_DIR_ARG.to_string());
        forwarded.push(data_dir.to_string_lossy().into_owned());
    }
    forwarded.push(OPEN_ACK_ARG.to_string());
    forwarded.push(ack.path.to_string_lossy().into_owned());

    let mut child = if let Some(path) = env_override()? {
        spawn_direct(&path, &forwarded)?
    } else {
        spawn_platform(&forwarded)?
    };
    wait_for_ack(&mut child, &ack.path, ack_timeout())
}

fn ack_timeout() -> Duration {
    std::env::var("DATAZEN_OPEN_ACK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|millis| *millis > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_ACK_TIMEOUT)
}

struct AckFile {
    path: PathBuf,
}

impl AckFile {
    fn create() -> Result<Self, CliError> {
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for attempt in 0..32 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos());
            let dir = base.join(format!("datazen-open-{pid}-{nonce}-{attempt}"));
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    return Ok(Self {
                        path: dir.join("ack"),
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(CliError::new(
                        E_INTERNAL,
                        format!("cannot create the open acknowledgement path: {error}"),
                    ));
                }
            }
        }
        Err(CliError::new(
            E_INTERNAL,
            "cannot create a unique open acknowledgement path",
        ))
    }
}

impl Drop for AckFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn wait_for_ack(child: &mut Child, ack: &Path, timeout: Duration) -> Result<(), CliError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                return Err(CliError::new(
                    E_EXECUTOR,
                    format!(
                        "desktop launcher exited with {status} before acknowledging the open request"
                    ),
                ));
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(error) => {
                return Err(CliError::new(
                    E_INTERNAL,
                    format!("cannot observe the desktop launcher: {error}"),
                ));
            }
        }
        if ack.is_file() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(CliError::new(
                E_TIMEOUT,
                format!(
                    "desktop did not acknowledge the open request within {} ms",
                    timeout.as_millis()
                ),
            ));
        }
        std::thread::sleep(ACK_POLL_INTERVAL);
    }
}

fn spawn_platform(forwarded: &[String]) -> Result<Child, CliError> {
    #[cfg(target_os = "macos")]
    {
        return macos::launch(forwarded);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::launch(forwarded);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::launch(forwarded);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = forwarded;
        Err(CliError::new(
            crate::error::E_UNSUPPORTED,
            "opening the DataZen desktop app is not supported on this platform",
        ))
    }
}

fn env_override() -> Result<Option<PathBuf>, CliError> {
    let Some(path) = std::env::var_os("DATAZEN_DESKTOP_BIN") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    if path.as_os_str().is_empty() {
        return Err(CliError::new(E_NOT_FOUND, "DATAZEN_DESKTOP_BIN is empty"));
    }
    if !path.is_file() {
        return Err(CliError::new(
            E_NOT_FOUND,
            format!("desktop executable does not exist: {}", path.display()),
        ));
    }
    Ok(Some(path))
}

fn spawn_direct(path: &Path, forwarded: &[String]) -> Result<Child, CliError> {
    Command::new(path)
        .args(forwarded)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            CliError::new(
                E_NOT_FOUND,
                format!("cannot launch the DataZen desktop app: {error}"),
            )
        })
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub(super) fn launch(forwarded: &[String]) -> Result<Child, CliError> {
        let app = app_candidates()
            .into_iter()
            .find(|path| path.is_dir())
            .ok_or_else(|| {
                CliError::new(
                    E_NOT_FOUND,
                    "DataZen.app was not found in /Applications or ~/Applications; \
                     set DATAZEN_DESKTOP_BIN to launch a development build",
                )
            })?;
        Command::new("/usr/bin/open")
            .arg("-n")
            .arg("-a")
            .arg(&app)
            .arg("--args")
            .args(forwarded)
            .spawn()
            .map_err(|error| {
                CliError::new(
                    E_NOT_FOUND,
                    format!("cannot run /usr/bin/open for {}: {error}", app.display()),
                )
            })
    }

    fn app_candidates() -> Vec<PathBuf> {
        let mut candidates = vec![PathBuf::from("/Applications/DataZen.app")];
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(PathBuf::from(home).join("Applications/DataZen.app"));
        }
        candidates
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub(super) fn launch(forwarded: &[String]) -> Result<Child, CliError> {
        let Some(path) = candidate_paths().into_iter().find(|path| path.is_file()) else {
            return Err(CliError::new(
                E_NOT_FOUND,
                "DataZen.exe was not found in the usual install locations; \
                 set DATAZEN_DESKTOP_BIN to launch a development build",
            ));
        };
        spawn_direct(&path, forwarded)
    }

    fn candidate_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for variable in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"] {
            let Some(base) = std::env::var_os(variable) else {
                continue;
            };
            let base = PathBuf::from(base);
            paths.push(base.join("Programs/DataZen/DataZen.exe"));
            paths.push(base.join("DataZen/DataZen.exe"));
        }
        paths
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub(super) fn launch(forwarded: &[String]) -> Result<Child, CliError> {
        let candidates = [
            PathBuf::from("/usr/bin/datazen-desktop"),
            PathBuf::from("/usr/local/bin/datazen-desktop"),
            PathBuf::from("/opt/DataZen/datazen-desktop"),
        ];
        let Some(path) = candidates.into_iter().find(|path| path.is_file()) else {
            return Err(CliError::new(
                E_NOT_FOUND,
                "datazen-desktop was not found in /usr/bin, /usr/local/bin, or /opt/DataZen; \
                 set DATAZEN_DESKTOP_BIN to launch a development build",
            ));
        };
        spawn_direct(&path, forwarded)
    }
}
