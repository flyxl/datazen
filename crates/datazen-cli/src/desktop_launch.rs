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
//! use the platform application locations below. macOS tests can point
//! `DATAZEN_DESKTOP_APP` at a temporary `.app` bundle.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{CliError, E_EXECUTOR, E_INTERNAL, E_NOT_FOUND, E_TIMEOUT, E_UNSUPPORTED};

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

    let LaunchedProcess {
        mut child,
        used_open_fallback,
    } = if let Some(path) = env_override()? {
        LaunchedProcess {
            child: spawn_direct(&path, &forwarded)?,
            used_open_fallback: false,
        }
    } else {
        spawn_platform(&forwarded)?
    };
    wait_for_launch_ack(&mut child, &ack.path, ack_timeout(), used_open_fallback)
}

fn wait_for_launch_ack(
    child: &mut Child,
    ack: &Path,
    timeout: Duration,
    used_open_fallback: bool,
) -> Result<(), CliError> {
    match wait_for_ack(child, ack, timeout) {
        Ok(()) => Ok(()),
        Err(_error) if used_open_fallback => Err(CliError::new(
            E_UNSUPPORTED,
            "macOS /usr/bin/open fallback could not verify the open acknowledgement",
        )),
        Err(error) => Err(error),
    }
}

struct LaunchedProcess {
    child: Child,
    used_open_fallback: bool,
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

fn spawn_platform(forwarded: &[String]) -> Result<LaunchedProcess, CliError> {
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

    pub(super) fn launch(forwarded: &[String]) -> Result<LaunchedProcess, CliError> {
        let app = app_path()?;
        if let Some(executable) = bundle_executable(&app) {
            // Directly spawning the bundle executable is the only path that
            // reliably preserves argv when the app is already running; `open`
            // may hand the launch to an existing process and lose arguments.
            return Ok(LaunchedProcess {
                child: spawn_direct(&executable, forwarded)?,
                used_open_fallback: false,
            });
        }

        // Defensive fallback for a malformed bundle. It is only acceptable
        // when the launch carries the ack file: `wait_for_ack` must be able to
        // prove that the desktop consumed the request.
        if !forwarded.iter().any(|arg| arg == OPEN_ACK_ARG) {
            return Err(CliError::new(
                E_UNSUPPORTED,
                "cannot verify a DataZen launch without --datazen-open-ack",
            ));
        }
        Command::new("/usr/bin/open")
            .arg("-n")
            .arg("-a")
            .arg(&app)
            .arg("--args")
            .args(forwarded)
            .spawn()
            .map(|child| LaunchedProcess {
                child,
                used_open_fallback: true,
            })
            .map_err(|error| {
                CliError::new(
                    E_UNSUPPORTED,
                    format!(
                        "cannot launch {} directly or via /usr/bin/open: {error}",
                        app.display()
                    ),
                )
            })
    }

    fn app_path() -> Result<PathBuf, CliError> {
        if let Some(path) = std::env::var_os("DATAZEN_DESKTOP_APP") {
            let path = PathBuf::from(path);
            if path.is_dir() {
                return Ok(path);
            }
            return Err(CliError::new(
                E_NOT_FOUND,
                "DATAZEN_DESKTOP_APP does not point at a .app directory",
            ));
        }
        app_candidates()
            .into_iter()
            .find(|path| path.is_dir())
            .ok_or_else(|| {
                CliError::new(
                    E_NOT_FOUND,
                    "DataZen.app was not found in /Applications or ~/Applications; \
                     set DATAZEN_DESKTOP_BIN to launch a development build",
                )
            })
    }

    /// Resolve `Contents/MacOS/<exe>` without relying on LaunchServices.
    /// Known Tauri binary names win; a custom `CFBundleExecutable` name is
    /// found by taking the first regular file in `Contents/MacOS`.
    fn bundle_executable(app: &Path) -> Option<PathBuf> {
        use std::os::unix::fs::PermissionsExt;

        fn executable_file(path: &Path) -> bool {
            path.metadata()
                .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        }

        let macos = app.join("Contents/MacOS");
        for name in ["datazen_desktop", "datazen"] {
            let candidate = macos.join(name);
            if executable_file(&candidate) {
                return Some(candidate);
            }
        }
        std::fs::read_dir(&macos).ok()?.find_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                return None;
            }
            let path = entry.path();
            executable_file(&path).then_some(path)
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

    pub(super) fn launch(forwarded: &[String]) -> Result<LaunchedProcess, CliError> {
        let Some(path) = candidate_paths().into_iter().find(|path| path.is_file()) else {
            return Err(CliError::new(
                E_NOT_FOUND,
                "DataZen.exe was not found in the usual install locations; \
                 set DATAZEN_DESKTOP_BIN to launch a development build",
            ));
        };
        Ok(LaunchedProcess {
            child: spawn_direct(&path, forwarded)?,
            used_open_fallback: false,
        })
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

    pub(super) fn launch(forwarded: &[String]) -> Result<LaunchedProcess, CliError> {
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
        Ok(LaunchedProcess {
            child: spawn_direct(&path, forwarded)?,
            used_open_fallback: false,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn failing_child() -> Child {
        Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 1")
            .spawn()
            .expect("spawn failing child")
    }

    #[test]
    fn open_fallback_ack_failure_is_unsupported() {
        let mut child = failing_child();
        let ack = std::env::temp_dir().join(format!("datazen-missing-ack-{}", std::process::id()));
        let error = wait_for_launch_ack(&mut child, &ack, Duration::from_millis(200), true)
            .expect_err("failing fallback must not succeed");
        assert_eq!(error.code, E_UNSUPPORTED);
        let _ = child.wait();
    }

    #[test]
    fn open_fallback_accepts_ack_file() {
        let ack = std::env::temp_dir().join(format!("datazen-valid-ack-{}", std::process::id()));
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "printf '%s\\n' '{{\"ok\":true}}' > '{}'",
                ack.display()
            ))
            .spawn()
            .expect("spawn ack-writing child");
        wait_for_launch_ack(&mut child, &ack, Duration::from_secs(2), true)
            .expect("ack file must satisfy the fallback");
        let _ = child.wait();
        let _ = std::fs::remove_file(&ack);
    }

    #[test]
    fn direct_ack_failure_keeps_executor_error() {
        let mut child = failing_child();
        let ack =
            std::env::temp_dir().join(format!("datazen-missing-direct-ack-{}", std::process::id()));
        let error = wait_for_launch_ack(&mut child, &ack, Duration::from_millis(200), false)
            .expect_err("failing direct launch must not succeed");
        assert_eq!(error.code, E_EXECUTOR);
        let _ = child.wait();
    }
}
