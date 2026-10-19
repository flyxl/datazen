//! POSIX process ownership. No shell, PATH search, inherited env or stdin.
use super::*;
use std::{
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::Stdio,
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    time::Instant,
};

fn signal_group(group: i32, signal: i32) {
    // SAFETY: group is the positive pid returned for OUR process_group(0) child.
    // A negative pid targets only that group, never a connection/profile scan.
    if group > 0 {
        unsafe {
            libc::kill(-group, signal);
        }
    }
}
fn group_absent(group: i32) -> bool {
    // SAFETY: signal zero only checks existence; EPERM is not absence.
    unsafe {
        libc::kill(-group, 0) == -1
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }
}
fn set_cleanup(cleanup: &Mutex<HashMap<String, NativeCleanup>>, key: &str, value: NativeCleanup) {
    cleanup
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.into(), value);
}
async fn reap(child: &mut Child, group: i32, grace: Duration) -> bool {
    signal_group(group, libc::SIGTERM);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        let reaped = child.try_wait().ok().flatten().is_some();
        if reaped && group_absent(group) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    signal_group(group, libc::SIGKILL);
    if !matches!(
        tokio::time::timeout(Duration::from_secs(2), child.wait()).await,
        Ok(Ok(_))
    ) {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if group_absent(group) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
pub(super) async fn run(
    key: String,
    installation: Arc<Installation>,
    cleanup: Arc<Mutex<HashMap<String, NativeCleanup>>>,
    cancel: Arc<AtomicBool>,
    limits: NativeToolLimits,
) -> Result<NativeToolReport, PortError> {
    if limits.max_output_bytes == 0
        || limits.max_output_bytes > 1_048_576
        || limits.timeout_ms == 0
        || limits.timeout_ms > 300_000
        || limits.terminate_grace_ms > 5000
    {
        return Err(denied());
    }
    if cancel.load(Ordering::Acquire) {
        return Err(denied());
    }
    // Copy the verified bytes to a private executable snapshot: replacing the
    // deployment path after hashing cannot switch which program we execute.
    DesktopNativeTools::verify(&installation.config)?;
    let bytes = std::fs::read(&installation.config.executable).map_err(|_| denied())?;
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != installation.config.sha256 {
        return Err(denied());
    }
    let directory = tempfile::Builder::new()
        .prefix("datazen-native-")
        .tempdir()
        .map_err(|_| denied())?;
    let executable = directory.path().join("tool");
    if std::fs::write(&executable, bytes).is_err()
        || std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o500)).is_err()
    {
        return no_process_cleanup(directory, key, cleanup);
    }
    let mut command = Command::new(&executable);
    command
        .arg("--version")
        .current_dir(directory.path())
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command.as_std_mut().process_group(0);
    if cancel.load(Ordering::Acquire) {
        return no_process_cleanup(directory, key, cleanup);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return no_process_cleanup(directory, key, cleanup),
    };
    set_cleanup(&cleanup, &key, NativeCleanup::Unconfirmed);
    // Child id and the two requested pipes are invariant after a successful spawn;
    // a missing value is post-spawn Unknown, never a pre-spawn Err.
    let group = child.id().and_then(|id| i32::try_from(id).ok());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut report = NativeToolReport {
        termination: NativeTermination::Failed,
        exit_code: None,
        stdout_bytes: 0,
        stderr_bytes: 0,
        cleanup: NativeCleanup::Unconfirmed,
        version_verified: false,
    };
    let Some(group) = group else {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Ok(report);
    };
    if let (Some(mut stdout), Some(mut stderr)) = (stdout, stderr) {
        let deadline = Instant::now() + Duration::from_millis(limits.timeout_ms);
        let mut out = [0_u8; 4096];
        let mut err = [0_u8; 4096];
        let mut output = Vec::new();
        let mut out_done = false;
        let mut err_done = false;
        loop {
            if cancel.load(Ordering::Acquire) {
                report.termination = NativeTermination::Cancelled;
                break;
            }
            if Instant::now() >= deadline {
                report.termination = NativeTermination::TimedOut;
                break;
            }
            if report.stdout_bytes.saturating_add(report.stderr_bytes)
                > limits.max_output_bytes as u64
            {
                report.termination = NativeTermination::OutputLimit;
                break;
            }
            match child.try_wait() {
                Ok(Some(status)) if out_done && err_done => {
                    report.termination = NativeTermination::Exited;
                    report.exit_code = status.code();
                    report.version_verified = status.success()
                        && std::str::from_utf8(&output).is_ok_and(|value| {
                            value.trim() == installation.config.expected_version_line
                        });
                    break;
                }
                Err(_) => break,
                _ => {}
            }
            tokio::select! {
                read = stdout.read(&mut out), if !out_done => match read {
                    Ok(0) => out_done = true,
                    Ok(count) => { report.stdout_bytes += count as u64; if output.len() + count <= 1024 { output.extend_from_slice(&out[..count]); } else { output.clear(); report.termination = NativeTermination::OutputLimit; break; } },
                    Err(_) => break,
                },
                read = stderr.read(&mut err), if !err_done => match read {
                    Ok(0) => err_done = true,
                    Ok(count) => report.stderr_bytes += count as u64,
                    Err(_) => break,
                },
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
    }
    // Closing both read pipes is intentional on overflow/cancel: no writer can
    // keep our reader blocked. Always terminate residual group members and reap.
    let reaped = reap(
        &mut child,
        group,
        Duration::from_millis(limits.terminate_grace_ms),
    )
    .await;
    if reaped {
        let path = directory.path().to_owned();
        if directory.close().is_ok() {
            report.cleanup = NativeCleanup::Confirmed;
            set_cleanup(&cleanup, &key, NativeCleanup::Confirmed);
        } else {
            tokio::spawn(async move {
                remove_directory(&path).await;
                set_cleanup(&cleanup, &key, NativeCleanup::Confirmed);
            });
        }
    } else {
        // Retain actual child AND private directory, independent of request
        // cancellation. Retry cleanup only, never application execution.
        tokio::spawn(async move {
            while !reap(&mut child, group, Duration::from_millis(100)).await {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let path = directory.path().to_owned();
            if directory.close().is_err() {
                remove_directory(&path).await;
            }
            set_cleanup(&cleanup, &key, NativeCleanup::Confirmed);
        });
    }
    Ok(report)
}

async fn remove_directory(path: &std::path::Path) {
    loop {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
        }
    }
}

// Even failed/cancelled preparation owns its private files until deletion is
// proved. Err means no process and no retained temporary resource.
fn no_process_cleanup(
    directory: tempfile::TempDir,
    key: String,
    cleanup: Arc<Mutex<HashMap<String, NativeCleanup>>>,
) -> Result<NativeToolReport, PortError> {
    let path = directory.path().to_owned();
    if directory.close().is_ok() {
        return Err(denied());
    }
    set_cleanup(&cleanup, &key, NativeCleanup::Unconfirmed);
    tokio::spawn(async move {
        remove_directory(&path).await;
        set_cleanup(&cleanup, &key, NativeCleanup::Confirmed);
    });
    Ok(NativeToolReport {
        termination: NativeTermination::Failed,
        exit_code: None,
        stdout_bytes: 0,
        stderr_bytes: 0,
        cleanup: NativeCleanup::Unconfirmed,
        version_verified: false,
    })
}
