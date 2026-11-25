//! T026-S2 independent audit: `BackupWindow` execution actions +
//! three-column rendering (`BackupWindow.tsx`).
//!
//! Written against the React sources (`BackupWindow.tsx`,
//! `lib/backupProgress.ts`, `lib/sqlFileExecution.ts`, the dialect
//! tables, the driver metas, `ConfirmDialog`), not against the
//! coder's tests. It pins the gaps the coder's 54 tests leave open:
//! the log pump's byte-boundary truncation on multi-byte lines, the
//! `format-custom` → `.dump` extension the dialects no longer offer,
//! the restore-progress `None`-payload path, the re-arm head, and
//! the badge tones / confirm dialog the coder only spot-checked.
//!
//! Finding F1 (BLOCKING): `audit_long_multibyte_log_line_…`
//! fails on the tested commit — `trim_line` slices a `&str` at byte
//! 4096, which panics when that byte falls inside a multi-byte
//! character (deterministic for an all-CJK line: 4096 = 1365*3+1).
//! See the audit report; the remaining tests are green.
//!
//! Retest note: F1 fixed by 98fa993db (char-boundary back-off in
//! `trim_line`); this test now passes — see the retest report.

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::{
    append_progress_log, apply_restore_progress, begin_backup, finish_backup,
    format_restore_progress, restore_progress_line, BackupConnection, BackupLogPump,
    BackupProgress, BackupProgressPhase, BackupWindow, BackupWindowEdits, BackupWindowVm,
    RestoreConfirm,
};

const DESIGN: (f32, f32) = (1440.0, 816.0);

fn conn(id: &str, name: &str, db_type: &str) -> BackupConnection {
    BackupConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        ..BackupConnection::default()
    }
}

fn progress(current: u64, total: u64, name: &str, phase: BackupProgressPhase) -> BackupProgress {
    BackupProgress {
        current,
        total,
        object_name: name.to_owned(),
        phase,
    }
}

// ---------------------------------------------------------------------------
// Pure logic: the log pump's byte boundary (finding F1)
// ---------------------------------------------------------------------------

#[test]
fn audit_long_multibyte_log_line_truncates_without_panicking() {
    // React `trimLine` slices by UTF-16 code unit; the Rust mirror
    // slices `&line[..4096]` by byte. A 2000-character CJK line is
    // 6000 bytes, and byte 4096 lands inside character 1366
    // (bytes 4095..4098) — slicing there must truncate at a char
    // boundary, never panic.
    let long = "中".repeat(2000);
    let lines = append_progress_log(&[], &long);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].ends_with("… (truncated)"),
        "over-long line is marked truncated"
    );
    assert!(
        lines[0].len() <= 4096 + "… (truncated)".len(),
        "truncated line stays within the byte budget: {}",
        lines[0].len()
    );
    assert!(
        lines[0].starts_with("中"),
        "the kept head is still the original text"
    );

    // The same straddle with an ASCII head: byte 4095 is 'x', bytes
    // 4096..4098 are one CJK character.
    let mixed = format!("{}{}", "x".repeat(4095), "中".repeat(100));
    let lines = append_progress_log(&[], &mixed);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].ends_with("… (truncated)"));
}

#[test]
fn audit_long_ascii_line_truncates_at_the_byte_boundary() {
    // Control for the test above: the ASCII case the coder already
    // covers keeps working — 5000 'x' become 4096 + the marker.
    let long = "x".repeat(5000);
    let lines = append_progress_log(&[], &long);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), 4096 + "… (truncated)".len());
}

// ---------------------------------------------------------------------------
// Pure logic: run-state machine gaps
// ---------------------------------------------------------------------------

#[test]
fn audit_format_custom_still_maps_to_the_dump_extension() {
    // React `handleBackup` still honours `format-custom` → `.dump`
    // even though no dialect offers it any more (S1 keeps the
    // branch); gzip wins over it.
    let mut vm = BackupWindowVm::default();
    vm.enabled_options.insert("format-custom".to_owned());
    assert_eq!(vm.file_extension(), "dump");
    assert_eq!(vm.default_file_name(), "untitled.dump");
    vm.compress_gzip = true;
    assert_eq!(vm.file_extension(), "gz");
    assert_eq!(vm.default_file_name(), "untitled.sql.gz");
}

#[test]
fn audit_apply_restore_progress_without_payload_keeps_progress() {
    // React `runSqlFileExecution` calls `onProgress(null, line)` —
    // a null payload leaves the progress bar's state alone and only
    // the status line moves.
    let mut vm = BackupWindowVm::default();
    let mut pump = BackupLogPump::new();
    let payload = progress(1, 2, "orders", BackupProgressPhase::Object);
    let line = restore_progress_line(&payload);
    apply_restore_progress(&mut vm, &mut pump, Some(&payload), &line);
    assert_eq!(vm.progress, Some(payload));
    apply_restore_progress(&mut vm, &mut pump, None, "still working");
    assert_eq!(
        vm.progress,
        Some(progress(1, 2, "orders", BackupProgressPhase::Object)),
        "a null payload keeps the last progress"
    );
    assert_eq!(vm.status_message, "still working");
    assert_eq!(
        vm.progress_log.last().map(String::as_str),
        Some("still working")
    );

    // The host helper is exactly the formatter the listener uses.
    assert_eq!(
        restore_progress_line(&progress(2, 0, "orders", BackupProgressPhase::Object)),
        format_restore_progress(&progress(2, 0, "orders", BackupProgressPhase::Object)),
    );
    assert_eq!(
        restore_progress_line(&progress(2, 0, "orders", BackupProgressPhase::Object)),
        "正在恢复 orders（2/…）"
    );
}

#[test]
fn audit_begin_backup_rearms_cleanly_after_a_finished_run() {
    // A second run starts from a clean head even after the first
    // run finished: the log resets to the single preparing line.
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("sess-1".to_owned());
    vm.selected_db = Some("app".to_owned());
    let mut pump = BackupLogPump::new();
    begin_backup(&mut vm, &mut pump);
    finish_backup(&mut vm, &mut pump, true);
    assert_eq!(
        vm.progress_log.last().map(String::as_str),
        Some(t("backup.success"))
    );
    begin_backup(&mut vm, &mut pump);
    assert!(vm.backing);
    assert_eq!(vm.progress, None);
    assert_eq!(vm.status_message, t("backup.progressPreparing"));
    assert_eq!(vm.progress_log, [t("backup.progressPreparing")]);
}

// ---------------------------------------------------------------------------
// Runtime: badge tones + confirm dialog (coder only spot-checked)
// ---------------------------------------------------------------------------

struct AuditProbe {
    vm: BackupWindowVm,
    edits: BackupWindowEdits,
    dark: bool,
    restore_confirm: Option<RestoreConfirm>,
}

impl AuditProbe {
    fn new(vm: BackupWindowVm) -> Self {
        let edits = BackupWindowEdits::new(&vm);
        Self {
            vm,
            edits,
            dark: true,
            restore_confirm: None,
        }
    }
}

fn audit_view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    // Only the edit bindings are wired (a field without `on_edit`
    // cannot take focus in a debug probe); every action callback
    // stays unwired, like the coder's `unwired_view`.
    let mut builder = BackupWindow::<AuditProbe>::new(s.vm.clone(), &s.edits)
        .on_file_name_edit(|st: &mut AuditProbe| st.edits.file_name_edit())
        .on_search_conn_edit(|st: &mut AuditProbe| st.edits.search_conn_edit())
        .on_search_db_edit(|st: &mut AuditProbe| st.edits.search_db_edit());
    if let Some(confirm) = &s.restore_confirm {
        builder = builder.restore_confirm(confirm.clone());
    }
    builder.build(s.dark)
}

fn runtime(probe: AuditProbe, size: (f32, f32)) -> Runtime<AuditProbe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(
        probe,
        audit_view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<AuditProbe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AuditProbe>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditProbe>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} has text"))
}

fn any_text(rt: &Runtime<AuditProbe>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

#[test]
fn audit_badge_tones_cover_all_five_tone_classes() {
    // The coder's badge test only renders the success and neutral
    // tones. Every `iconBg` in the driver metas maps to a tone in
    // `db_type_badge`: warning / accent / success / danger /
    // neutral — all five must show on the tile.
    let mut vm = BackupWindowVm::default();
    vm.apply_connections_loaded(
        vec![
            conn("my", "MySQL", "mysql"),
            conn("pg", "Postgres", "postgresql"),
            conn("lite", "SQLite", "sqlite"),
            conn("ms", "SQL Server", "sqlserver"),
            conn("kw", "Kiwi", "kiwi"),
        ],
        vec![],
    );
    let rt = runtime(AuditProbe::new(vm), DESIGN);
    let k = kit(true);
    assert_eq!(
        element(&rt, "backup-connection-badge-my").fill,
        k.c.warning.fg,
        "mysql bg-warning renders the warning tone"
    );
    assert_eq!(
        element(&rt, "backup-connection-badge-pg").fill,
        k.c.bg_accent,
        "postgresql bg-accent renders the accent tone"
    );
    assert_eq!(
        element(&rt, "backup-connection-badge-lite").fill,
        k.c.success.fg,
        "sqlite bg-success renders the success tone"
    );
    assert_eq!(
        element(&rt, "backup-connection-badge-ms").fill,
        k.c.danger.fg,
        "sqlserver bg-danger renders the danger tone"
    );
    assert_eq!(
        element(&rt, "backup-connection-badge-kw").fill,
        k.c.text_faint,
        "an unknown type renders the neutral DB tile"
    );
}

#[test]
fn audit_restore_confirm_dialog_renders_title_message_and_buttons() {
    // `ConfirmDialog` (`packages/ui/src/ConfirmDialog.tsx`): ghost
    // cancel + danger confirm with the default `common.*` labels,
    // `max-w-lg` (512pt), and the warning triangle — the Rust
    // dialog must carry the same content and keys.
    let mut probe = AuditProbe::new(BackupWindowVm::new(true));
    probe.restore_confirm = Some(RestoreConfirm {
        title: t("common.restoreDatabase").to_owned(),
        message:
            "目标数据库「app」已有 3 个对象。继续将先删除现有表/视图，再写入备份内容。是否覆盖？"
                .to_owned(),
        epoch: 1,
    });
    let rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "backup-restore-confirm"));
    assert_eq!(
        text_of(&rt, "backup-restore-confirm-title"),
        t("common.restoreDatabase")
    );
    assert!(rt.elements().iter().any(|e| e.text.as_deref()
        == Some(
            "目标数据库「app」已有 3 个对象。继续将先删除现有表/视图，再写入备份内容。是否覆盖？"
        )));
    assert!(has_key(&rt, "confirm-dialog-cancel"));
    assert!(has_key(&rt, "confirm-dialog-ok"));
    // The labels live on the buttons' text children, like the
    // coder's `any_text` uses elsewhere.
    assert!(any_text(&rt, t("common.cancel")));
    assert!(any_text(&rt, t("common.confirm")));

    // No confirm pending: nothing mounts.
    let rt = runtime(AuditProbe::new(BackupWindowVm::new(true)), DESIGN);
    assert!(!has_key(&rt, "backup-restore-confirm"));
    assert!(!has_key(&rt, "confirm-dialog-cancel"));
}

#[test]
fn audit_log_copy_button_only_renders_with_a_nonempty_log() {
    // React renders `<ProgressLog>` (with its copy button) only
    // when `progressLog.length > 0`; the empty case is a spacer.
    let mut probe = AuditProbe::new(BackupWindowVm::default());
    let rt = runtime(probe, DESIGN);
    assert!(!has_key(&rt, "backup-progress-log-copy"));
    assert!(has_key(&rt, "backup-progress-log-spacer"));

    let mut vm = BackupWindowVm::default();
    vm.progress_log = vec!["第一行".to_owned()];
    probe = AuditProbe::new(vm);
    let rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "backup-progress-log-copy"));
    assert!(any_text(&rt, t("backup.copyLog")));
    assert!(!has_key(&rt, "backup-progress-log-spacer"));
}
