//! T026-S2: `BackupWindow` — the three-column rendering, the chrome,
//! the start button, and the backup/restore runs (`BackupWindow.tsx`).
//!
//! Pure-logic tests call the progress / pump / run functions
//! directly; runtime tests render the window in a probe `Runtime`
//! and read back the laid-out element tree — never the component's
//! own report of itself. The probe host scripts the async middle
//! (the fake `connect`, the fake `backup_database` /
//! `restore_sql_file` invokes, the progress replays, the confirm
//! dialog answers) so the three backup paths (success / cancel /
//! error) and the restore paths (confirm / decline / no tables /
//! error) run end to end through real presses. Every assertion is
//! checked against the React sources named in the module docs.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use datazen_ui::i18n::{has, t};
use datazen_ui::ui::badge::BadgeTone;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::{
    abort_restore_before_execute, append_progress_log, apply_backup_progress,
    apply_restore_progress, backup_progress_ratio, begin_backup, begin_restore, db_badge_meta,
    fail_run, finish_backup, finish_restore, format_backup_progress, format_restore_progress,
    restore_progress_line, BackupConnection, BackupLogPump, BackupProgress, BackupProgressPhase,
    BackupWindow, BackupWindowEdits, BackupWindowVm, RestoreConfirm, SelectConnectionOutcome,
    SessionConnected, MAX_PROGRESS_LOG_CHARS, MAX_PROGRESS_LOG_LINES,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn conn(id: &str, name: &str, db_type: &str) -> BackupConnection {
    BackupConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        ..BackupConnection::default()
    }
}

fn conn_full(
    id: &str,
    name: &str,
    db_type: &str,
    group: Option<&str>,
    host: Option<&str>,
    database: Option<&str>,
) -> BackupConnection {
    BackupConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        host: host.map(str::to_owned),
        database: database.map(str::to_owned),
        group: group.map(str::to_owned),
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
// Probe host
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct BackupCall {
    default_file_name: String,
    filter_extension: String,
    options: Vec<String>,
    compress: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RestoreCall {
    db_session_id: String,
    database: String,
    options: Vec<String>,
}

struct Probe {
    vm: BackupWindowVm,
    edits: BackupWindowEdits,
    pump: BackupLogPump,
    dark: bool,
    backup_calls: Vec<BackupCall>,
    restore_calls: Vec<RestoreCall>,
    emitted_refresh: Vec<String>,
    copied_log: Vec<String>,
    confirm_epoch: u64,
    restore_confirm: Option<RestoreConfirm>,
    pending_restore: bool,
    pending_preferred: Option<String>,
    fake_session_id: String,
    fake_server_version: Option<String>,
    fake_databases: Vec<String>,
    backup_plan: Vec<BackupProgress>,
    backup_saved: bool,
    backup_error: Option<String>,
    restore_tables: usize,
    restore_plan: Vec<BackupProgress>,
    restore_executed: bool,
    restore_error: Option<String>,
}

impl Probe {
    fn new(vm: BackupWindowVm) -> Self {
        let edits = BackupWindowEdits::new(&vm);
        Probe {
            vm,
            edits,
            pump: BackupLogPump::new(),
            dark: true,
            backup_calls: Vec::new(),
            restore_calls: Vec::new(),
            emitted_refresh: Vec::new(),
            copied_log: Vec::new(),
            confirm_epoch: 0,
            restore_confirm: None,
            pending_restore: false,
            pending_preferred: None,
            fake_session_id: "sess-1".to_owned(),
            fake_server_version: Some("15.2".to_owned()),
            fake_databases: vec!["app".to_owned(), "other".to_owned()],
            backup_plan: Vec::new(),
            backup_saved: true,
            backup_error: None,
            restore_tables: 0,
            restore_plan: Vec::new(),
            restore_executed: true,
            restore_error: None,
        }
    }
}

/// The host's `handleSelectConnection`: reset + connect + apply, with
/// the N4 first leg — `default_database` is `preferred ??
/// conn.database`, resolved by the caller.
fn select_connection(st: &mut Probe, conn: BackupConnection) {
    match st.vm.begin_select_connection(&conn) {
        SelectConnectionOutcome::UnsupportedType => {}
        SelectConnectionOutcome::NeedsConnect { .. } => {
            let default_database = st.pending_preferred.take().or(conn.database.clone());
            st.vm.apply_session_connected(SessionConnected {
                db_session_id: st.fake_session_id.clone(),
                server_version: st.fake_server_version.clone(),
                databases: st.fake_databases.clone(),
                default_database,
            });
        }
    }
}

/// The host's connections load, with the N4 second leg —
/// `take_prefill` after every load.
fn load_and_prefill(
    st: &mut Probe,
    conns: Vec<BackupConnection>,
    groups: Vec<String>,
    connection_id: Option<&str>,
    database: Option<&str>,
) {
    st.vm.apply_connections_loaded(conns, groups);
    if let Some(target) = st.vm.take_prefill(connection_id, database) {
        st.pending_preferred = target.preferred_database.clone();
        select_connection(st, target.connection);
    }
}

/// The host's `datazen:connection-closed` listener, with the N4
/// third leg — a missing payload maps to `""`.
fn connection_closed(st: &mut Probe, payload: Option<&str>) {
    st.vm.on_connection_closed(payload.unwrap_or(""));
}

/// The host's `handleBackup`: sync the edits, compute the file
/// name, arm the run, replay the scripted `backup-progress`
/// payloads, invoke the fake `backup_database`, and finish or fail.
fn start_backup(st: &mut Probe) {
    st.edits.sync_into_vm(&mut st.vm);
    st.backup_calls.push(BackupCall {
        default_file_name: st.vm.default_file_name(),
        filter_extension: st.vm.file_extension().to_owned(),
        options: st.vm.enabled_options.iter().cloned().collect(),
        compress: st.vm.compress_gzip,
    });
    begin_backup(&mut st.vm, &mut st.pump);
    let plan = std::mem::take(&mut st.backup_plan);
    for payload in &plan {
        apply_backup_progress(&mut st.vm, &mut st.pump, payload);
    }
    if let Some(message) = st.backup_error.take() {
        fail_run(&mut st.vm, &mut st.pump, &message);
        return;
    }
    finish_backup(&mut st.vm, &mut st.pump, st.backup_saved);
}

/// The host's `handleRestore` up to the confirm gate: arm the run,
/// then suspend at the dialog when the database is non-empty.
fn start_restore(st: &mut Probe) {
    begin_restore(&mut st.vm, &mut st.pump);
    if st.restore_tables > 0 {
        let database = st.vm.selected_db.clone().unwrap_or_default();
        st.confirm_epoch += 1;
        st.restore_confirm = Some(RestoreConfirm {
            title: t("common.restoreDatabase").to_owned(),
            message: t("backup.restoreOverwriteConfirm")
                .replace("{database}", &database)
                .replace("{count}", &st.restore_tables.to_string()),
            epoch: st.confirm_epoch,
        });
        st.pending_restore = true;
        return;
    }
    finish_restore_flow(st, Vec::new());
}

/// The host's confirm answer: resume the restore with `overwrite`.
fn confirm_restore(st: &mut Probe) {
    st.restore_confirm = None;
    st.pending_restore = false;
    finish_restore_flow(st, vec!["overwrite".to_owned()]);
}

/// The host's cancel answer: abort before executing.
fn cancel_restore(st: &mut Probe) {
    st.restore_confirm = None;
    st.pending_restore = false;
    abort_restore_before_execute(&mut st.vm);
}

/// The rest of `runSqlFileExecution` after the confirm gate: replay
/// the scripted `restore-progress` payloads, invoke the fake
/// `restore_sql_file`, finish or fail, and emit on success.
fn finish_restore_flow(st: &mut Probe, options: Vec<String>) {
    let plan = std::mem::take(&mut st.restore_plan);
    for payload in &plan {
        let line = restore_progress_line(payload);
        apply_restore_progress(&mut st.vm, &mut st.pump, Some(payload), &line);
    }
    // The invoke is attempted before it can throw, so the call is
    // recorded either way; only the emit waits for success.
    st.restore_calls.push(RestoreCall {
        db_session_id: st.vm.db_session_id.clone().unwrap_or_default(),
        database: st.vm.selected_db.clone().unwrap_or_default(),
        options,
    });
    if let Some(message) = st.restore_error.take() {
        // `runSqlFileExecution`'s catch pushes first, then
        // `handleRestore`'s catch pushes again — the pump dedupes.
        st.pump.push(&message);
        st.vm.progress_log = st.pump.lines().to_vec();
        fail_run(&mut st.vm, &mut st.pump, &message);
        return;
    }
    let executed = st.restore_executed;
    finish_restore(&mut st.vm, &mut st.pump, executed);
    if executed {
        if let Some(session) = st.vm.db_session_id.clone() {
            st.emitted_refresh.push(session);
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut builder = BackupWindow::<Probe>::new(s.vm.clone(), &s.edits)
        .on_select_connection(select_connection)
        .on_select_database(|st: &mut Probe, name| st.vm.selected_db = Some(name))
        .on_toggle_group(|st: &mut Probe, group| st.vm.toggle_group(&group))
        .on_toggle_option(|st: &mut Probe, id| st.vm.toggle_option(&id))
        .on_toggle_dropdown(|st: &mut Probe| {
            st.vm.option_dropdown_open = !st.vm.option_dropdown_open;
        })
        .on_close_dropdown(|st: &mut Probe| st.vm.option_dropdown_open = false)
        .on_toggle_compress(|st: &mut Probe| st.vm.compress_gzip = !st.vm.compress_gzip)
        .on_start_backup(start_backup)
        .on_start_restore(start_restore)
        .on_copy_log(|st: &mut Probe| st.copied_log.push(st.vm.progress_log.join("\n")))
        .on_confirm_restore(confirm_restore)
        .on_cancel_restore(cancel_restore)
        .on_file_name_edit(|st: &mut Probe| st.edits.file_name_edit())
        .on_search_conn_edit(|st: &mut Probe| st.edits.search_conn_edit())
        .on_search_db_edit(|st: &mut Probe| st.edits.search_db_edit());
    if let Some(confirm) = &s.restore_confirm {
        builder = builder.restore_confirm(confirm.clone());
    }
    builder.build(s.dark)
}

fn unwired_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    // Only the edit bindings are wired (a field without `on_edit`
    // cannot take focus, so a fully unwired view cannot even build
    // in a debug probe); every action callback stays unwired.
    BackupWindow::<Probe>::new(s.vm.clone(), &s.edits)
        .on_file_name_edit(|st: &mut Probe| st.edits.file_name_edit())
        .on_search_conn_edit(|st: &mut Probe| st.edits.search_conn_edit())
        .on_search_db_edit(|st: &mut Probe| st.edits.search_db_edit())
        .build(s.dark)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_unwired(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, unwired_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} has text"))
}

fn any_text<S>(rt: &Runtime<S>, text: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(text))
}

fn kit_roots<S>(rt: &Runtime<S>) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("kit-root"))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = element(rt, key).rect;
    click_at(rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
}

fn click_at<S>(rt: &mut Runtime<S>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn type_into<S>(rt: &mut Runtime<S>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.settle_all();
    rt.frame(0.0);
}

/// Replace a field's whole text through the UI: focus, move to the
/// end, delete back over the current text, then paste. A plain
/// `paste` inserts at the cursor, so typing over `"untitled"`
/// without this would append to it.
fn replace_text_in<S>(rt: &mut Runtime<S>, key: &str, current_len: usize, text: &str) {
    click(rt, key);
    press(rt, Key::End);
    for _ in 0..current_len {
        press(rt, Key::Backspace);
    }
    rt.paste(text);
    rt.settle_all();
    rt.frame(0.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        if e.exiting {
            continue;
        }
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

/// A probe with a loaded pg/mysql/redis trio, the `team` group
/// declared, and the pg connection selected with its databases.
fn loaded_probe() -> Probe {
    let mut probe = Probe::new(BackupWindowVm::default());
    load_and_prefill(
        &mut probe,
        vec![
            conn_full(
                "pg",
                "Main PG",
                "postgresql",
                Some("team"),
                Some("db.internal"),
                Some("app"),
            ),
            conn_full(
                "my",
                "Legacy MySQL",
                "mysql",
                Some("team"),
                None,
                Some("app"),
            ),
            conn_full("kv", "Cache", "redis", None, Some("cache.internal"), None),
        ],
        vec!["team".to_owned()],
        None,
        None,
    );
    probe
}

fn select_pg(probe: &mut Probe) {
    let pg = conn_full(
        "pg",
        "Main PG",
        "postgresql",
        Some("team"),
        Some("db.internal"),
        Some("app"),
    );
    select_connection(probe, pg);
}

// ---------------------------------------------------------------------------
// Pure logic: i18n keys
// ---------------------------------------------------------------------------

#[test]
fn the_i18n_keys_the_slice_asks_for_resolve() {
    for key in [
        "backup.fileName",
        "backup.fileNameHint",
        "backup.searchConnection",
        "backup.searchDatabase",
        "backup.selectConnectionFirst",
        "backup.addOption",
        "backup.compressGzip",
        "backup.progressLog",
        "backup.copyLog",
        "backup.startBackup",
        "backup.startRestore",
        "backup.inProgress",
        "backup.restoring",
        "backup.progressPreparing",
        "backup.success",
        "backup.restoreSuccess",
        "backup.restorePreparing",
        "backup.progressObject",
        "backup.progressWriting",
        "backup.restoreProgress",
        "backup.logOmitted",
        "common.backupDatabase",
        "common.restoreDatabase",
        "backup.restoreOverwriteConfirm",
        "common.cancel",
        "common.confirm",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

// ---------------------------------------------------------------------------
// Pure logic: progress formatting
// ---------------------------------------------------------------------------

#[test]
fn format_backup_progress_covers_all_phases() {
    assert_eq!(
        format_backup_progress(&progress(0, 0, "", BackupProgressPhase::Writing)),
        t("backup.progressWriting")
    );
    assert_eq!(
        format_backup_progress(&progress(0, 0, "", BackupProgressPhase::Done)),
        t("backup.success")
    );
    assert_eq!(
        format_backup_progress(&progress(3, 9, "orders", BackupProgressPhase::Object)),
        "正在导出 orders（3/9）"
    );
    assert_eq!(
        format_backup_progress(&progress(3, 0, "orders", BackupProgressPhase::Object)),
        t("backup.progressPreparing"),
        "no total yet: preparing"
    );
    assert_eq!(
        format_backup_progress(&progress(3, 9, "", BackupProgressPhase::Object)),
        t("backup.progressPreparing"),
        "no object name yet: preparing"
    );
}

#[test]
fn format_restore_progress_covers_all_phases() {
    assert_eq!(
        format_restore_progress(&progress(0, 0, "", BackupProgressPhase::Done)),
        t("backup.restoreSuccess")
    );
    assert_eq!(
        format_restore_progress(&progress(0, 0, "", BackupProgressPhase::Writing)),
        t("backup.restorePreparing")
    );
    assert_eq!(
        format_restore_progress(&progress(2, 5, "orders", BackupProgressPhase::Object)),
        "正在恢复 orders（2/5）"
    );
    assert_eq!(
        format_restore_progress(&progress(2, 0, "orders", BackupProgressPhase::Object)),
        "正在恢复 orders（2/…）",
        "unknown total renders as …"
    );
    assert_eq!(
        format_restore_progress(&progress(0, 0, "", BackupProgressPhase::Object)),
        t("backup.restoring"),
        "no object name yet: restoring"
    );
}

#[test]
fn backup_progress_ratio_covers_all_phases() {
    assert_eq!(
        backup_progress_ratio(&progress(0, 0, "", BackupProgressPhase::Done)),
        1.0
    );
    assert_eq!(
        backup_progress_ratio(&progress(0, 0, "", BackupProgressPhase::Writing)),
        0.95
    );
    assert_eq!(
        backup_progress_ratio(&progress(0, 0, "", BackupProgressPhase::Object)),
        0.0,
        "nothing counted yet"
    );
    // 0.12 + log10(10) / 5 = 0.32.
    assert!(
        (backup_progress_ratio(&progress(9, 0, "", BackupProgressPhase::Object)) - 0.32).abs()
            < 1e-9
    );
    assert_eq!(
        backup_progress_ratio(&progress(u64::MAX, 0, "", BackupProgressPhase::Object)),
        0.9,
        "unknown-total growth caps at 0.9"
    );
    assert_eq!(
        backup_progress_ratio(&progress(1, 2, "", BackupProgressPhase::Object)),
        0.5
    );
    assert_eq!(
        backup_progress_ratio(&progress(9, 10, "", BackupProgressPhase::Object)),
        0.9,
        "9/10 = 0.9, under the 0.94 cap"
    );
    assert_eq!(
        backup_progress_ratio(&progress(20, 10, "", BackupProgressPhase::Object)),
        0.94,
        "over-total caps at 0.94"
    );
}

// ---------------------------------------------------------------------------
// Pure logic: log pump
// ---------------------------------------------------------------------------

#[test]
fn append_progress_log_dedupes_blanks_and_truncates() {
    let lines = append_progress_log(&[], "  ");
    assert!(lines.is_empty(), "blank lines are dropped");
    let lines = append_progress_log(&lines, "first");
    assert_eq!(lines, ["first"]);
    let lines = append_progress_log(&lines, "first");
    assert_eq!(lines, ["first"], "a repeat of the last line is dropped");
    let lines = append_progress_log(&lines, "  second  ");
    assert_eq!(lines, ["first", "second"], "lines are trimmed");
    let long = "x".repeat(5000);
    let lines = append_progress_log(&lines, &long);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[2].len(), 4096 + "… (truncated)".len());
    assert!(lines[2].starts_with("xxxx"));
    assert!(lines[2].ends_with("… (truncated)"));
}

#[test]
fn append_progress_log_caps_at_1500_lines_with_omitted_marker() {
    let mut lines: Vec<String> = (0..1600).map(|i| format!("line {i}")).collect();
    // One more append triggers the trim (1601 lines).
    lines = append_progress_log(&lines, "line 1600");
    assert_eq!(lines.len(), MAX_PROGRESS_LOG_LINES);
    assert_eq!(lines[0], "line 0");
    assert_eq!(lines[99], "line 99");
    // tailStart 202 − headEnd 100 = 102 omitted.
    assert_eq!(lines[100], "… 已省略 102 行 …");
    assert_eq!(lines[101], "line 202");
    assert_eq!(lines[lines.len() - 1], "line 1600");
}

#[test]
fn append_progress_log_char_budget_drops_oldest_head_first() {
    let mut lines: Vec<String> = (0..100).map(|_| "x".repeat(4000)).collect();
    lines = append_progress_log(&lines, "last");
    // 101 × 4000 chars over the 256 KiB budget: the tail is a single
    // line, so the head shrinks from 100 to 65 (65 × 4000 = 260000).
    assert_eq!(lines.len(), 67);
    assert_eq!(lines[65], "… 已省略 35 行 …");
    assert_eq!(lines[66], "last");
    let chars: usize = lines.iter().map(String::len).sum();
    assert!(
        chars <= MAX_PROGRESS_LOG_CHARS + "… 已省略 35 行 …".len(),
        "marker aside, the kept text fits the budget: {chars}"
    );
}

#[test]
fn append_progress_log_carries_an_earlier_marker_count_forward() {
    // A previously trimmed log (marker at 100, count 102) grows by
    // 200 lines; the second trim drops the old marker and absorbs
    // its count: 201 dropped + (102 − 1) = 302.
    let mut lines: Vec<String> = (0..1600).map(|i| format!("line {i}")).collect();
    lines = append_progress_log(&lines, "line 1600");
    assert_eq!(lines[100], "… 已省略 102 行 …");
    for i in 1601..1801 {
        lines = append_progress_log(&lines, &format!("line {i}"));
    }
    assert_eq!(lines.len(), MAX_PROGRESS_LOG_LINES);
    assert_eq!(lines[100], "… 已省略 302 行 …");
}

#[test]
fn log_pump_reset_push_flush_mirror_react() {
    let mut pump = BackupLogPump::new();
    pump.reset(vec!["a".to_owned()]);
    assert_eq!(pump.lines(), ["a"]);
    pump.push("b");
    assert_eq!(pump.lines(), ["a", "b"]);
    pump.push("b");
    assert_eq!(pump.lines(), ["a", "b"], "consecutive dup dropped");
    pump.push("   ");
    assert_eq!(pump.lines(), ["a", "b"], "blank dropped");
    pump.flush();
    assert_eq!(pump.lines(), ["a", "b"], "flush keeps the lines");
    pump.reset(Vec::new());
    assert!(pump.lines().is_empty());
}

// ---------------------------------------------------------------------------
// Pure logic: run-state machine
// ---------------------------------------------------------------------------

fn run_vm() -> (BackupWindowVm, BackupLogPump) {
    let mut vm = BackupWindowVm::default();
    vm.db_session_id = Some("sess-1".to_owned());
    vm.selected_db = Some("app".to_owned());
    (vm, BackupLogPump::new())
}

#[test]
fn backup_run_begin_apply_finish_success() {
    let (mut vm, mut pump) = run_vm();
    begin_backup(&mut vm, &mut pump);
    assert!(vm.backing);
    assert_eq!(vm.progress, None);
    assert_eq!(vm.status_message, t("backup.progressPreparing"));
    assert_eq!(vm.progress_log, [t("backup.progressPreparing")]);

    apply_backup_progress(
        &mut vm,
        &mut pump,
        &progress(1, 2, "orders", BackupProgressPhase::Object),
    );
    assert_eq!(vm.status_message, "正在导出 orders（1/2）");
    assert_eq!(
        vm.progress,
        Some(progress(1, 2, "orders", BackupProgressPhase::Object))
    );
    assert_eq!(
        vm.progress_log,
        [t("backup.progressPreparing"), "正在导出 orders（1/2）"]
    );

    finish_backup(&mut vm, &mut pump, true);
    assert!(!vm.backing);
    assert_eq!(vm.status_message, t("backup.success"));
    assert_eq!(
        vm.progress,
        Some(progress(0, 0, "", BackupProgressPhase::Done))
    );
    assert_eq!(
        vm.progress_log.last().map(String::as_str),
        Some(t("backup.success"))
    );
}

#[test]
fn backup_run_cancel_clears_status_progress_and_log() {
    let (mut vm, mut pump) = run_vm();
    begin_backup(&mut vm, &mut pump);
    apply_backup_progress(
        &mut vm,
        &mut pump,
        &progress(1, 2, "orders", BackupProgressPhase::Object),
    );
    finish_backup(&mut vm, &mut pump, false);
    assert!(!vm.backing);
    assert_eq!(vm.status_message, "");
    assert_eq!(vm.progress, None);
    assert!(vm.progress_log.is_empty());
}

#[test]
fn backup_run_failure_reports_the_error() {
    let (mut vm, mut pump) = run_vm();
    begin_backup(&mut vm, &mut pump);
    fail_run(&mut vm, &mut pump, "disk full");
    assert!(!vm.backing);
    assert_eq!(vm.status_message, "disk full");
    assert_eq!(vm.progress, None);
    assert_eq!(
        vm.progress_log,
        [t("backup.progressPreparing"), "disk full"]
    );
}

#[test]
fn restore_run_begin_apply_finish_success() {
    let (mut vm, mut pump) = run_vm();
    begin_restore(&mut vm, &mut pump);
    assert!(vm.backing);
    assert_eq!(vm.status_message, t("backup.restoring"));
    assert_eq!(vm.progress_log, [t("backup.restoring")]);

    let payload = progress(1, 2, "orders", BackupProgressPhase::Object);
    let line = restore_progress_line(&payload);
    apply_restore_progress(&mut vm, &mut pump, Some(&payload), &line);
    assert_eq!(vm.status_message, "正在恢复 orders（1/2）");
    assert_eq!(vm.progress, Some(payload));

    finish_restore(&mut vm, &mut pump, true);
    assert!(!vm.backing);
    assert_eq!(vm.status_message, t("backup.restoreSuccess"));
    assert_eq!(
        vm.progress,
        Some(progress(0, 0, "", BackupProgressPhase::Done))
    );
    assert_eq!(
        vm.progress_log.last().map(String::as_str),
        Some(t("backup.restoreSuccess"))
    );
}

#[test]
fn restore_run_cancelled_file_dialog_clears() {
    let (mut vm, mut pump) = run_vm();
    begin_restore(&mut vm, &mut pump);
    finish_restore(&mut vm, &mut pump, false);
    assert!(!vm.backing);
    assert_eq!(vm.status_message, "");
    assert_eq!(vm.progress, None);
    assert!(vm.progress_log.is_empty());
}

#[test]
fn restore_run_declined_confirm_only_clears_backing() {
    let (mut vm, mut pump) = run_vm();
    begin_restore(&mut vm, &mut pump);
    abort_restore_before_execute(&mut vm);
    assert!(!vm.backing);
    // The log, status, and progress stay as begin_restore left them:
    // runSqlFileExecution returns before its own reset.
    assert_eq!(vm.status_message, t("backup.restoring"));
    assert_eq!(vm.progress_log, [t("backup.restoring")]);
    assert_eq!(vm.progress, None);
}

#[test]
fn restore_run_failure_pushes_the_message_once() {
    let (mut vm, mut pump) = run_vm();
    begin_restore(&mut vm, &mut pump);
    pump.push("boom");
    vm.progress_log = pump.lines().to_vec();
    fail_run(&mut vm, &mut pump, "boom");
    assert_eq!(vm.status_message, "boom");
    assert_eq!(vm.progress, None);
    assert_eq!(
        vm.progress_log,
        [t("backup.restoring"), "boom"],
        "the inner and outer catch push the same line; the pump dedupes"
    );
}

// ---------------------------------------------------------------------------
// Pure logic: edits + badge table
// ---------------------------------------------------------------------------

#[test]
fn edits_sync_into_vm_and_reset_from_vm() {
    let vm = BackupWindowVm::default();
    let mut edits = BackupWindowEdits::new(&vm);
    assert_eq!(edits.file_name.text, "untitled");
    assert_eq!(edits.search_conn.text, "");
    assert_eq!(edits.search_db.text, "");

    edits.file_name.replace_text("mydb".to_owned());
    edits.search_conn.replace_text("app".to_owned());
    let mut vm = vm;
    edits.sync_into_vm(&mut vm);
    assert_eq!(vm.file_name, "mydb");
    assert_eq!(vm.search_conn, "app");
    assert_eq!(vm.search_db, "");

    vm.file_name = "reset".to_owned();
    edits.reset_from_vm(&vm);
    assert_eq!(edits.file_name.text, "reset");
    assert_eq!(edits.file_name_edit().text, "reset");
    assert_eq!(edits.search_conn_edit().text, "app");
    assert_eq!(edits.search_db_edit().text, "");
}

#[test]
fn db_badge_meta_matches_the_driver_metas() {
    for (db_type, label, tone) in [
        ("mysql", "My", BadgeTone::Warning),
        ("mariadb", "Ma", BadgeTone::Warning),
        ("doris", "Do", BadgeTone::Warning),
        ("starrocks", "Sr", BadgeTone::Warning),
        ("manticore", "Mc", BadgeTone::Warning),
        ("ob_oracle", "Ob", BadgeTone::Warning),
        ("postgresql", "Pg", BadgeTone::Accent),
        ("questdb", "Qd", BadgeTone::Accent),
        ("cloudberry", "Cb", BadgeTone::Accent),
        ("sqlite", "Lt", BadgeTone::Success),
        ("rqlite", "Rq", BadgeTone::Accent),
        ("turso", "Tu", BadgeTone::Success),
        ("sqlserver", "Ss", BadgeTone::Danger),
        ("clickhouse", "Ch", BadgeTone::Warning),
        ("duckdb", "Dk", BadgeTone::Accent),
        ("redis", "Rd", BadgeTone::Danger),
        ("mongodb", "Mb", BadgeTone::Success),
        ("elasticsearch", "Es", BadgeTone::Warning),
        ("hbase", "Hb", BadgeTone::Warning),
        ("influxdb", "If", BadgeTone::Warning),
        ("vector", "Vc", BadgeTone::Success),
        ("victoriametrics", "Vm", BadgeTone::Accent),
    ] {
        assert_eq!(db_badge_meta(db_type), (label, tone), "{db_type}");
    }
    assert_eq!(db_badge_meta("presto"), ("DB", BadgeTone::Neutral));
    assert_eq!(db_badge_meta(""), ("DB", BadgeTone::Neutral));
}

// ---------------------------------------------------------------------------
// Runtime: chrome + three columns
// ---------------------------------------------------------------------------

#[test]
fn chrome_title_and_three_columns_render_in_dark_and_light() {
    for dark in [true, false] {
        let mut probe = loaded_probe();
        probe.dark = dark;
        let rt = runtime(probe, DESIGN);
        assert_eq!(
            text_of(&rt, "backup-window-title"),
            t("common.backupDatabase")
        );
        assert!(has_key(&rt, "backup-file-name-row"));
        assert!(has_key(&rt, "backup-connections"));
        assert!(has_key(&rt, "backup-databases"));
        assert!(has_key(&rt, "backup-options"));
        assert_eq!(kit_roots(&rt), 1);
    }
}

#[test]
fn restore_mode_has_no_file_name_row_and_uses_restore_labels() {
    let probe = Probe::new(BackupWindowVm::new(true));
    let rt = runtime(probe, DESIGN);
    assert_eq!(
        text_of(&rt, "backup-window-title"),
        t("common.restoreDatabase")
    );
    assert!(!has_key(&rt, "backup-file-name-row"));
    assert!(!has_key(&rt, "backup-file-name"));
    assert!(any_text(&rt, t("backup.startRestore")));
    assert!(!has_key(&rt, "backup-start-backup"));
}

#[test]
fn typing_in_the_connection_search_filters_without_a_host_sync() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    assert!(has_key(&rt, "backup-connection-row-pg"));
    assert!(has_key(&rt, "backup-connection-row-my"));
    type_into(&mut rt, "backup-search-connection", "pg");
    // No host step ran: the builder overlays the edit text onto its
    // render-time Vm clone, so the filter applies on the next frame.
    assert!(has_key(&rt, "backup-connection-row-pg"));
    assert!(!has_key(&rt, "backup-connection-row-my"));
    assert_eq!(rt.state.edits.search_conn.text, "pg");
    assert_eq!(rt.state.vm.search_conn, "");
}

#[test]
fn clicking_a_connection_loads_its_databases_and_selects_the_default() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-my");
    assert_eq!(rt.state.vm.selected_conn_id.as_deref(), Some("my"));
    assert!(has_key(&rt, "backup-db-row-app"));
    assert!(has_key(&rt, "backup-db-row-other"));
    assert_eq!(rt.state.vm.selected_db.as_deref(), Some("app"));
    // The selected row is the accent one, at 20% (React's
    // `bg-accent/20`).
    let kit = kit(true);
    assert_eq!(
        element(&rt, "backup-db-row-app").fill,
        kit.alpha(kit.c.bg_accent, 0.2)
    );
    // The db type + version tag shows the connected type.
    assert!(any_text(&rt, "MySQL 15.2"));
    assert!(has_key(&rt, "backup-db-type"));
}

#[test]
fn unsupported_connection_rows_are_dimmed_and_statused() {
    let mut probe = Probe::new(BackupWindowVm::default());
    load_and_prefill(
        &mut probe,
        vec![conn("kv", "Cache", "redis")],
        vec![],
        None,
        None,
    );
    let mut rt = runtime(probe, DESIGN);
    assert_eq!(element(&rt, "backup-connection-row-kv").opacity, 0.5);
    click(&mut rt, "backup-connection-row-kv");
    // S1 selects the row first, then the unsupported branch sets
    // the status and clears the session — the row stays selected,
    // like React, which sets the id before the type check.
    assert_eq!(rt.state.vm.selected_conn_id.as_deref(), Some("kv"));
    assert_eq!(rt.state.vm.db_session_id, None);
    assert_eq!(rt.state.vm.status_message, t("backup.unsupportedType"));
}

#[test]
fn group_header_collapses_and_expands_its_connections() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    assert!(has_key(&rt, "backup-connection-row-pg"));
    click(&mut rt, "backup-group-header-team");
    assert!(!has_key(&rt, "backup-connection-row-pg"));
    assert!(!has_key(&rt, "backup-connection-row-my"));
    // The ungrouped redis row is outside the group and stays.
    assert!(has_key(&rt, "backup-connection-row-kv"));
    click(&mut rt, "backup-group-header-team");
    assert!(has_key(&rt, "backup-connection-row-pg"));
    assert!(has_key(&rt, "backup-connection-row-my"));
}

#[test]
fn empty_connection_list_shows_no_rows() {
    let rt = runtime(Probe::new(BackupWindowVm::default()), DESIGN);
    assert!(!has_key(&rt, "backup-connection-row-pg"));
    assert!(has_key(&rt, "backup-connections"));
    assert!(has_key(&rt, "backup-search-connection"));
}

#[test]
fn database_list_filter_yields_an_empty_list_not_an_empty_state() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-pg");
    assert!(has_key(&rt, "backup-db-row-app"));
    type_into(&mut rt, "backup-search-database", "zzz");
    assert!(!has_key(&rt, "backup-db-row-app"));
    assert!(!has_key(&rt, "backup-db-row-other"));
    // React shows the empty hint only when there are no databases at
    // all (or no connection): a filter that matches nothing leaves
    // the list simply empty.
    assert!(!has_key(&rt, "backup-db-empty"));
}

#[test]
fn database_list_with_no_databases_shows_the_hint() {
    let mut probe = loaded_probe();
    probe.fake_databases = Vec::new();
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-connection-row-pg");
    assert!(has_key(&rt, "backup-db-empty"));
    assert!(any_text(&rt, t("backup.selectConnectionFirst")));
}

#[test]
fn no_connection_selected_shows_the_select_first_hint() {
    let rt = runtime(Probe::new(BackupWindowVm::default()), DESIGN);
    assert!(has_key(&rt, "backup-db-empty"));
    assert!(any_text(&rt, t("backup.selectConnectionFirst")));
}

// ---------------------------------------------------------------------------
// Runtime: options column
// ---------------------------------------------------------------------------

#[test]
fn option_dropdown_opens_toggles_chips_and_removes() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-my");
    // mysql defaults: routines + triggers on.
    assert!(has_key(&rt, "backup-option-chip-routines"));
    assert!(has_key(&rt, "backup-option-chip-triggers"));

    click(&mut rt, "backup-add-option");
    assert!(has_key(&rt, "backup-options-panel"));
    assert!(has_key(&rt, "backup-option-no-data"));
    assert!(has_key(&rt, "backup-option-add-drop-table"));
    assert!(has_key(&rt, "backup-option-single-transaction"));
    assert!(has_key(&rt, "backup-option-routines"));
    assert!(has_key(&rt, "backup-option-triggers"));
    assert!(has_key(&rt, "backup-option-no-create-info"));

    // Toggling an off option adds its chip; toggling again removes it.
    click(&mut rt, "backup-option-no-data");
    assert!(has_key(&rt, "backup-option-chip-no-data"));
    click(&mut rt, "backup-option-no-data");
    assert!(!has_key(&rt, "backup-option-chip-no-data"));

    // Escape closes the panel. The panel overlays the chips while
    // open, so the chip's remove button is only reachable once it
    // is closed — close it before clicking the chip.
    press(&mut rt, Key::Escape);
    assert!(!has_key(&rt, "backup-options-panel"));

    // Removing a chip from the row unchecks it in the panel.
    click(&mut rt, "backup-option-chip-remove-routines");
    assert!(!has_key(&rt, "backup-option-chip-routines"));
    assert!(!rt.state.vm.enabled_options.contains("routines"));
}

#[test]
fn option_dropdown_closes_when_the_trigger_is_pressed_again() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-my");
    click(&mut rt, "backup-add-option");
    assert!(has_key(&rt, "backup-options-panel"));
    click(&mut rt, "backup-add-option");
    assert!(!has_key(&rt, "backup-options-panel"));
}

#[test]
fn compress_checkbox_toggles_and_only_renders_when_backing_a_db() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-my");
    assert!(has_key(&rt, "backup-compress-gzip"));
    assert!(!rt.state.vm.compress_gzip);
    click(&mut rt, "backup-compress-gzip");
    assert!(rt.state.vm.compress_gzip);
    click(&mut rt, "backup-compress-gzip");
    assert!(!rt.state.vm.compress_gzip);
}

#[test]
fn progress_bar_geometry_matches_the_ratio() {
    // A mid-run state, set directly: 1/2 → the bar fills half the track.
    let mut probe = loaded_probe();
    probe.vm.status_message = "正在导出 orders（1/2）".to_owned();
    probe.vm.progress = Some(progress(1, 2, "orders", BackupProgressPhase::Object));
    probe.vm.backing = false;
    let rt = runtime(probe, DESIGN);
    let track = element(&rt, "backup-progress-track").rect;
    let bar = element(&rt, "backup-progress-bar").rect;
    assert!(track[2] > 100.0, "track has real width: {track:?}");
    let ratio = bar[2] / track[2];
    assert!(
        (ratio - 0.5).abs() < 0.05,
        "bar/track = {ratio}, track {track:?}, bar {bar:?}"
    );

    // Indeterminate (backing, no progress yet) sits at 1/3.
    let mut probe = loaded_probe();
    probe.vm.status_message = t("backup.progressPreparing").to_owned();
    probe.vm.backing = true;
    let rt = runtime(probe, DESIGN);
    let track = element(&rt, "backup-progress-track").rect;
    let bar = element(&rt, "backup-progress-bar").rect;
    let ratio = bar[2] / track[2];
    assert!((ratio - 1.0 / 3.0).abs() < 0.05, "bar/track = {ratio}");
}

#[test]
fn status_message_and_log_render_and_the_log_is_a_spacer_when_empty() {
    let mut probe = loaded_probe();
    probe.vm.status_message = "备份成功".to_owned();
    probe.vm.progress_log = vec!["第一行".to_owned(), "第二行".to_owned()];
    let rt = runtime(probe, DESIGN);
    assert_eq!(text_of(&rt, "backup-status-text"), "备份成功");
    assert!(has_key(&rt, "backup-progress-log-line-0"));
    assert_eq!(text_of(&rt, "backup-progress-log-line-0"), "第一行");
    assert_eq!(text_of(&rt, "backup-progress-log-line-1"), "第二行");
    assert!(!has_key(&rt, "backup-progress-log-spacer"));

    let rt = runtime(loaded_probe(), DESIGN);
    assert!(!has_key(&rt, "backup-status"));
    assert!(!has_key(&rt, "backup-progress-log-line-0"));
    assert!(has_key(&rt, "backup-progress-log-spacer"));
}

// ---------------------------------------------------------------------------
// Runtime: backup journeys
// ---------------------------------------------------------------------------

#[test]
fn backup_success_calls_backup_database_with_the_right_args() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    probe.backup_plan = vec![
        progress(1, 2, "orders", BackupProgressPhase::Object),
        progress(2, 2, "orders", BackupProgressPhase::Object),
    ];
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-backup");
    assert_eq!(rt.state.backup_calls.len(), 1);
    assert_eq!(
        rt.state.backup_calls[0],
        BackupCall {
            default_file_name: "untitled.sql".to_owned(),
            filter_extension: "sql".to_owned(),
            options: vec!["routines".to_owned(), "triggers".to_owned()],
            compress: false,
        }
    );
    assert!(!rt.state.vm.backing);
    assert_eq!(rt.state.vm.status_message, t("backup.success"));
    assert_eq!(
        rt.state.vm.progress_log,
        [
            t("backup.progressPreparing"),
            "正在导出 orders（1/2）",
            "正在导出 orders（2/2）",
            t("backup.success"),
        ]
    );
    assert_eq!(
        rt.state.vm.progress,
        Some(progress(0, 0, "", BackupProgressPhase::Done))
    );
    assert!(any_text(&rt, t("backup.success")));
}

#[test]
fn backup_with_gzip_uses_the_gz_extension() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    probe.vm.compress_gzip = true;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-backup");
    assert_eq!(
        rt.state.backup_calls[0].default_file_name,
        "untitled.sql.gz"
    );
    assert_eq!(rt.state.backup_calls[0].filter_extension, "gz");
    assert!(rt.state.backup_calls[0].compress);
}

#[test]
fn backup_with_a_typed_file_name_uses_it() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    let mut rt = runtime(probe, DESIGN);
    // Replace the default "untitled" (8 chars) with "mydb" through
    // the UI: focus, End, delete back, paste.
    replace_text_in(&mut rt, "backup-file-name", "untitled".len(), "mydb");
    assert_eq!(rt.state.edits.file_name.text, "mydb");
    click(&mut rt, "backup-start-backup");
    assert_eq!(rt.state.backup_calls[0].default_file_name, "mydb.sql");
}

#[test]
fn backup_cancel_clears_status_progress_and_log() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    probe.backup_saved = false;
    probe.backup_plan = vec![progress(1, 2, "orders", BackupProgressPhase::Object)];
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-backup");
    assert_eq!(rt.state.backup_calls.len(), 1);
    assert_eq!(rt.state.vm.status_message, "");
    assert_eq!(rt.state.vm.progress, None);
    assert!(rt.state.vm.progress_log.is_empty());
    assert!(!has_key(&rt, "backup-status"));
    assert!(has_key(&rt, "backup-progress-log-spacer"));
}

#[test]
fn backup_error_reports_the_message() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    probe.backup_error = Some("disk full".to_owned());
    probe.backup_plan = vec![progress(1, 2, "orders", BackupProgressPhase::Object)];
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-backup");
    assert_eq!(rt.state.vm.status_message, "disk full");
    assert_eq!(rt.state.vm.progress, None);
    assert_eq!(
        rt.state.vm.progress_log,
        [
            t("backup.progressPreparing"),
            "正在导出 orders（1/2）",
            "disk full",
        ]
    );
}

#[test]
fn start_button_is_disabled_until_a_session_and_a_database_are_ready() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    // Nothing selected: pressing start must not invoke anything.
    // (`ElementInfo.inert` does not track the kit button's
    // disabled state, so the test asserts the behaviour — no
    // invoke — rather than the flag.)
    click(&mut rt, "backup-start-backup");
    assert!(rt.state.backup_calls.is_empty());
    // A connection is connected but no database is chosen: still disabled.
    // (A direct `rt.state` mutation does not mark the runtime
    // dirty — `frame` only re-runs the view when state changed —
    // so `settle_all` forces the rebuild after each mutation.)
    click(&mut rt, "backup-connection-row-my");
    rt.state.vm.selected_db = None;
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "backup-start-backup");
    assert!(rt.state.backup_calls.is_empty());
    // Session + database: enabled, and pressing invokes once.
    rt.state.vm.selected_db = Some("app".to_owned());
    rt.settle_all();
    rt.frame(0.0);
    click(&mut rt, "backup-start-backup");
    assert_eq!(rt.state.backup_calls.len(), 1);
}

#[test]
fn copy_log_copies_the_joined_lines() {
    let mut probe = loaded_probe();
    probe.vm.progress_log = vec!["第一行".to_owned(), "第二行".to_owned()];
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-progress-log-copy");
    assert_eq!(rt.state.copied_log, ["第一行\n第二行"]);
}

// ---------------------------------------------------------------------------
// Runtime: restore journeys
// ---------------------------------------------------------------------------

fn restore_probe() -> Probe {
    let mut probe = Probe::new(BackupWindowVm::new(true));
    load_and_prefill(
        &mut probe,
        vec![conn_full(
            "pg",
            "Main PG",
            "postgresql",
            None,
            Some("db.internal"),
            Some("app"),
        )],
        vec![],
        None,
        None,
    );
    select_pg(&mut probe);
    probe
}

#[test]
fn restore_with_tables_confirms_then_emits_refresh() {
    let mut probe = restore_probe();
    probe.restore_tables = 3;
    probe.restore_plan = vec![progress(1, 2, "orders", BackupProgressPhase::Object)];
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    // The dialog opens with the overwrite question.
    assert!(has_key(&rt, "backup-restore-confirm"));
    assert_eq!(
        text_of(&rt, "backup-restore-confirm-title"),
        t("common.restoreDatabase")
    );
    assert!(any_text(
        &rt,
        "目标数据库「app」已有 3 个对象。继续将先删除现有表/视图，再写入备份内容。是否覆盖？"
    ));
    assert!(rt.state.restore_calls.is_empty());
    assert!(rt.state.emitted_refresh.is_empty());

    click(&mut rt, "confirm-dialog-ok");
    assert!(!has_key(&rt, "backup-restore-confirm"));
    assert_eq!(
        rt.state.restore_calls,
        [RestoreCall {
            db_session_id: "sess-1".to_owned(),
            database: "app".to_owned(),
            options: vec!["overwrite".to_owned()],
        }]
    );
    assert_eq!(rt.state.emitted_refresh, ["sess-1"]);
    assert_eq!(rt.state.vm.status_message, t("backup.restoreSuccess"));
    assert_eq!(
        rt.state.vm.progress_log,
        [
            t("backup.restoring"),
            "正在恢复 orders（1/2）",
            t("backup.restoreSuccess"),
        ]
    );
}

#[test]
fn restore_confirm_declined_aborts_before_execute() {
    let mut probe = restore_probe();
    probe.restore_tables = 2;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    assert!(has_key(&rt, "backup-restore-confirm"));
    click(&mut rt, "confirm-dialog-cancel");
    assert!(!has_key(&rt, "backup-restore-confirm"));
    assert!(rt.state.restore_calls.is_empty());
    assert!(rt.state.emitted_refresh.is_empty());
    assert!(!rt.state.vm.backing);
    // React returns before the log reset: the restoring line stays.
    assert_eq!(rt.state.vm.status_message, t("backup.restoring"));
    assert_eq!(rt.state.vm.progress_log, [t("backup.restoring")]);
}

#[test]
fn restore_without_tables_skips_the_dialog() {
    let mut probe = restore_probe();
    probe.restore_tables = 0;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    assert!(!has_key(&rt, "backup-restore-confirm"));
    assert_eq!(rt.state.restore_calls.len(), 1);
    assert_eq!(rt.state.restore_calls[0].options, Vec::<String>::new());
    assert_eq!(rt.state.emitted_refresh, ["sess-1"]);
    assert_eq!(rt.state.vm.status_message, t("backup.restoreSuccess"));
}

#[test]
fn restore_cancelled_file_dialog_clears() {
    let mut probe = restore_probe();
    probe.restore_executed = false;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    assert_eq!(rt.state.restore_calls.len(), 1);
    assert!(rt.state.emitted_refresh.is_empty());
    assert_eq!(rt.state.vm.status_message, "");
    assert!(rt.state.vm.progress_log.is_empty());
}

#[test]
fn restore_error_reports_the_message_once_and_does_not_emit() {
    let mut probe = restore_probe();
    probe.restore_error = Some("boom".to_owned());
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    assert_eq!(rt.state.vm.status_message, "boom");
    assert_eq!(rt.state.vm.progress, None);
    assert_eq!(rt.state.vm.progress_log, [t("backup.restoring"), "boom"]);
    assert!(rt.state.emitted_refresh.is_empty());
    assert_eq!(rt.state.restore_calls.len(), 1, "the invoke was attempted");
}

// ---------------------------------------------------------------------------
// Runtime: host contract (N4) + geometry + unwired view
// ---------------------------------------------------------------------------

#[test]
fn prefill_after_load_selects_and_prefers_the_given_database() {
    let mut probe = Probe::new(BackupWindowVm::default());
    load_and_prefill(
        &mut probe,
        vec![
            conn_full("pg", "Main PG", "postgresql", None, None, Some("app")),
            conn_full("my", "Legacy MySQL", "mysql", None, None, None),
        ],
        vec![],
        Some("pg"),
        Some("other"),
    );
    assert_eq!(probe.vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(probe.vm.selected_db.as_deref(), Some("other"));
    let rt = runtime(probe, DESIGN);
    assert!(has_key(&rt, "backup-db-row-other"));
    assert!(has_key(&rt, "backup-db-row-app"));
}

#[test]
fn prefill_with_an_unknown_id_does_nothing_but_a_later_load_retries() {
    let mut probe = Probe::new(BackupWindowVm::default());
    let conns = vec![conn_full(
        "pg",
        "Main PG",
        "postgresql",
        None,
        None,
        Some("app"),
    )];
    load_and_prefill(
        &mut probe,
        conns.clone(),
        vec![],
        Some("ghost"),
        Some("app"),
    );
    assert_eq!(probe.vm.selected_conn_id, None);
    load_and_prefill(&mut probe, conns, vec![], Some("pg"), Some("app"));
    assert_eq!(probe.vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(probe.vm.selected_db.as_deref(), Some("app"));
}

#[test]
fn connection_closed_maps_a_missing_payload_to_empty_and_clears_only_that_session() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    // A close for another session is a no-op.
    connection_closed(&mut probe, Some("other-session"));
    assert_eq!(probe.vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(probe.vm.db_session_id.as_deref(), Some("sess-1"));
    // A close with a missing payload maps to "" — also a no-op.
    connection_closed(&mut probe, None);
    assert_eq!(probe.vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(probe.vm.db_session_id.as_deref(), Some("sess-1"));
    // A close for this session clears the session id only — React
    // clears `dbSessionId`, not the selected connection.
    connection_closed(&mut probe, Some("sess-1"));
    assert_eq!(probe.vm.selected_conn_id.as_deref(), Some("pg"));
    assert_eq!(probe.vm.db_session_id, None);
}

#[test]
fn full_window_at_the_minimum_size_stays_inside_the_window() {
    let rt = runtime(loaded_probe(), MIN);
    assert_in_window(&rt, MIN.0, MIN.1, "backup window 960x640");
}

#[test]
fn unwired_view_renders_and_its_presses_are_no_ops() {
    let mut probe = loaded_probe();
    select_pg(&mut probe);
    let mut rt = runtime_unwired(probe, DESIGN);
    assert!(has_key(&rt, "backup-connection-row-pg"));
    assert!(has_key(&rt, "backup-db-row-app"));
    click(&mut rt, "backup-connection-row-my");
    assert_eq!(rt.state.vm.selected_conn_id.as_deref(), Some("pg"));
    click(&mut rt, "backup-start-backup");
    assert!(rt.state.backup_calls.is_empty());
}

#[test]
fn clicking_a_database_row_selects_it() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-pg");
    assert_eq!(rt.state.vm.selected_db.as_deref(), Some("app"));
    click(&mut rt, "backup-db-row-other");
    assert_eq!(rt.state.vm.selected_db.as_deref(), Some("other"));
    // Move the pointer off the row first: a hovered row's fill is
    // the hover-blended color, and the hover fades on a motion
    // track — settle it before reading the plain accent wash.
    rt.pointer_move(20.0, 20.0);
    rt.settle_all();
    rt.frame(0.0);
    let kit = kit(true);
    assert_eq!(
        element(&rt, "backup-db-row-other").fill,
        kit.alpha(kit.c.bg_accent, 0.2)
    );
    assert_eq!(
        element(&rt, "backup-db-row-app").fill,
        rune::Color::TRANSPARENT,
        "the previously selected row loses its accent fill"
    );
}

#[test]
fn option_dropdown_closes_when_the_scrim_is_pressed() {
    let mut rt = runtime(loaded_probe(), DESIGN);
    click(&mut rt, "backup-connection-row-my");
    click(&mut rt, "backup-add-option");
    assert!(has_key(&rt, "backup-options-panel"));
    // The chrome title band (top-left) is outside the panel: the
    // press lands on the dismissal scrim.
    click_at(&mut rt, 20.0, 20.0);
    assert!(!has_key(&rt, "backup-options-panel"));
    assert!(!rt.state.vm.option_dropdown_open);
}

#[test]
fn db_badges_render_the_success_and_neutral_tones() {
    let mut probe = Probe::new(BackupWindowVm::default());
    load_and_prefill(
        &mut probe,
        vec![
            conn("lite", "Local File", "sqlite"),
            conn("presto", "Warehouse", "presto"),
        ],
        vec![],
        None,
        None,
    );
    let rt = runtime(probe, DESIGN);
    let kit = kit(true);
    assert_eq!(
        element(&rt, "backup-connection-badge-lite").fill,
        kit.c.success.fg,
        "sqlite renders the success tone"
    );
    assert_eq!(
        element(&rt, "backup-connection-badge-presto").fill,
        kit.c.text_faint,
        "an unknown type renders the neutral DB tile"
    );
    assert!(any_text(&rt, "Lt"));
    assert!(any_text(&rt, "DB"));
}

#[test]
fn restore_confirm_close_button_aborts_before_execute() {
    let mut probe = restore_probe();
    probe.restore_tables = 2;
    let mut rt = runtime(probe, DESIGN);
    click(&mut rt, "backup-start-restore");
    assert!(has_key(&rt, "backup-restore-confirm"));
    click(&mut rt, "backup-restore-confirm-close");
    assert!(!has_key(&rt, "backup-restore-confirm"));
    assert!(rt.state.restore_calls.is_empty());
    assert!(rt.state.emitted_refresh.is_empty());
    assert!(!rt.state.vm.backing);
    assert_eq!(rt.state.vm.status_message, t("backup.restoring"));
    assert_eq!(rt.state.vm.progress_log, [t("backup.restoring")]);
}
