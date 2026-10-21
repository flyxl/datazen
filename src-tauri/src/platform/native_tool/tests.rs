use super::*;
use std::{os::unix::fs::PermissionsExt, sync::atomic::Ordering, time::Duration};
fn fixture(source: &str) -> (tempfile::TempDir, DesktopNativeTools, NativeToolSelection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("probe");
    std::fs::write(&path, format!("#!/bin/sh\n{source}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o500)).unwrap();
    let sha256 = Sha256::digest(std::fs::read(&path).unwrap()).into();
    let selection = NativeToolSelection {
        tool_id: "postgres.pg-dump-version".into(),
        version: "synthetic-1".into(),
    };
    let tools = DesktopNativeTools::new(vec![NativeProbeInstallation {
        tool_id: selection.tool_id.clone(),
        version: selection.version.clone(),
        executable: path,
        sha256,
        expected_version_line: "pg_dump (PostgreSQL) synthetic-1".into(),
    }])
    .unwrap();
    (dir, tools, selection)
}
#[tokio::test]
async fn native_desktop_fixed_argv_clean_env_cwd_no_stdin_and_verified_output() {
    std::env::set_var("PATH_SENTINEL_SYNTHETIC", "synthetic-must-not-be-inherited");
    let (_dir, tools, selection) = fixture("test \"$#\" = 1 && test \"$1\" = --version || exit 4\ntest -z \"$PATH_SENTINEL_SYNTHETIC\" || exit 5\ntest \"$LANG\" = C && test \"$LC_ALL\" = C || exit 6\ntest -f ./tool || exit 7\nread ignored && exit 8\necho 'pg_dump (PostgreSQL) synthetic-1'");
    let resolved = tools.resolve(&selection).unwrap();
    assert_eq!(resolved.max_database_sockets, Some(0));
    let report = tools
        .run(
            "clean",
            resolved,
            Arc::new(AtomicBool::new(false)),
            NativeToolLimits::default(),
        )
        .await
        .unwrap();
    std::env::remove_var("PATH_SENTINEL_SYNTHETIC");
    assert_eq!(report.exit_code, Some(0));
    assert!(report.version_verified);
    assert_eq!(report.cleanup, NativeCleanup::Confirmed);
    assert_eq!(
        tools.cleanup_status("clean").await,
        NativeCleanup::Confirmed
    );
}
#[test]
fn native_desktop_deny_all_unknown_versions_paths_and_binary_changes() {
    let (_dir, tools, selection) = fixture("echo 'pg_dump (PostgreSQL) synthetic-1'");
    let mut foreign = selection.clone();
    foreign.tool_id = "/bin/sh".into();
    assert!(tools.resolve(&foreign).is_err());
    foreign = selection.clone();
    foreign.version = "unregistered".into();
    assert!(tools.resolve(&foreign).is_err());
    assert!(DesktopNativeTools::new(Vec::new())
        .unwrap()
        .resolve(&selection)
        .is_err());
    let installation = tools.installations.values().next().unwrap();
    std::fs::set_permissions(
        &installation.config.executable,
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::write(&installation.config.executable, "changed").unwrap();
    assert!(tools.resolve(&selection).is_err());
}
#[tokio::test]
async fn native_desktop_output_overflow_is_bounded_killed_and_reaped() {
    let (_dir, tools, selection) =
        fixture("while :; do printf 'synthetic-output'; printf 'synthetic-error' >&2; done");
    let limits = NativeToolLimits {
        max_output_bytes: 128,
        timeout_ms: 2000,
        terminate_grace_ms: 20,
    };
    let report = tools
        .run(
            "overflow",
            tools.resolve(&selection).unwrap(),
            Arc::new(AtomicBool::new(false)),
            limits,
        )
        .await
        .unwrap();
    assert_eq!(report.termination, NativeTermination::OutputLimit);
    assert_eq!(report.cleanup, NativeCleanup::Confirmed);
    assert!(!report.version_verified);
    assert!(report.stdout_bytes + report.stderr_bytes <= 128 + 8192);
}
#[tokio::test]
async fn native_desktop_cancel_group_and_reap_even_if_caller_stops_waiting() {
    let (_dir, tools, selection) =
        fixture("(trap '' TERM; while :; do :; done) &\ntrap '' TERM\nwhile :; do :; done");
    let tools = Arc::new(tools);
    let cancel = Arc::new(AtomicBool::new(false));
    let runner = tools.clone();
    let stop = cancel.clone();
    let task = tokio::spawn(async move {
        runner
            .run(
                "cancel",
                runner.resolve(&selection).unwrap(),
                stop,
                NativeToolLimits {
                    timeout_ms: 1000,
                    terminate_grace_ms: 20,
                    ..Default::default()
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        while !tools
            .cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key("cancel")
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    task.abort(); // provider task keeps ownership of the original process group.
    cancel.store(true, Ordering::Release);
    tokio::time::timeout(Duration::from_secs(5), async {
        while tools.cleanup_status("cancel").await != NativeCleanup::Confirmed {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn native_desktop_forged_resolution_and_unverified_version_fail_closed() {
    let (_dir, tools, selection) = fixture("echo 'unexpected synthetic version'");
    let mut forged = tools.resolve(&selection).unwrap();
    forged.max_database_sockets = Some(4);
    assert!(tools
        .run(
            "forged",
            forged,
            Arc::new(AtomicBool::new(false)),
            NativeToolLimits::default()
        )
        .await
        .is_err());
    assert!(!tools
        .cleanup
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key("forged"));
    let report = tools
        .run(
            "mismatch",
            tools.resolve(&selection).unwrap(),
            Arc::new(AtomicBool::new(false)),
            NativeToolLimits::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.exit_code, Some(0));
    assert!(!report.version_verified);
    assert_eq!(report.cleanup, NativeCleanup::Confirmed);
}
