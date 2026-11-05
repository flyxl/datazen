//! Cross-platform launch of the DataZen desktop host.
//!
//! The CLI never invents a deep link. It starts a real desktop binary with an
//! argv contract the desktop parses at startup:
//!
//! `--datazen-open-connection <id> [--datazen-open-query <sql>]`
//!
//! `DATAZEN_DESKTOP_BIN` is an explicit test/advanced override; normal installs
//! use the platform application locations below.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{CliError, E_NOT_FOUND};

const OPEN_CONNECTION_ARG: &str = "--datazen-open-connection";
const OPEN_QUERY_ARG: &str = "--datazen-open-query";

pub fn launch(connection_id: &str, query: Option<&str>) -> Result<(), CliError> {
    let mut forwarded = vec![OPEN_CONNECTION_ARG.to_string(), connection_id.to_string()];
    if let Some(query) = query {
        forwarded.push(OPEN_QUERY_ARG.to_string());
        forwarded.push(query.to_string());
    }

    if let Some(path) = env_override()? {
        return spawn_direct(&path, &forwarded);
    }

    #[cfg(target_os = "macos")]
    {
        return macos::launch(&forwarded);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::launch(&forwarded);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::launch(&forwarded);
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

fn spawn_direct(path: &Path, forwarded: &[String]) -> Result<(), CliError> {
    Command::new(path)
        .args(forwarded)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
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

    pub(super) fn launch(forwarded: &[String]) -> Result<(), CliError> {
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
        let status = Command::new("/usr/bin/open")
            .arg("-a")
            .arg(&app)
            .arg("--args")
            .args(forwarded)
            .status()
            .map_err(|error| {
                CliError::new(
                    E_NOT_FOUND,
                    format!("cannot run /usr/bin/open for {}: {error}", app.display()),
                )
            })?;
        if !status.success() {
            return Err(CliError::new(
                E_NOT_FOUND,
                format!("macOS could not launch {}", app.display()),
            ));
        }
        Ok(())
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

    pub(super) fn launch(forwarded: &[String]) -> Result<(), CliError> {
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

    pub(super) fn launch(forwarded: &[String]) -> Result<(), CliError> {
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
