//! T026-S2: the Backup window's execution wiring and three-column
//! rendering.
//!
//! Ported from `packages/frontend/src/windows/backup/BackupWindow.tsx`
//! (564 lines). S1 owns the state model and selection logic
//! ([`BackupWindowVm`]); this slice owns the rest:
//!
//! * the `WindowChromeSlot` title bar (backup / restore);
//! * the file-name row (backup only);
//! * column 1 — the grouped, searchable connection list;
//! * column 2 — the searchable database list with its empty states;
//! * column 3 — the type/version box, the option dropdown + chips,
//!   the gzip checkbox, the status message + progress bar, the
//!   progress log, and the start button;
//! * the restore confirm dialog (in the overlay layer);
//! * the run-state machine's host contract ([`super::backup_run`])
//!   and the progress formatting ([`super::backup_progress`]).
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim, for the
//! parts React names (`backup-status`, `backup-progress-bar`,
//! `backup-progress-log`, `backup-progress-log-copy`,
//! `backup-start-backup`, `backup-start-restore`,
//! `confirm-dialog-cancel`, `confirm-dialog-ok`); repeated rows
//! suffix the id (`backup-connection-row-{id}`,
//! `backup-db-row-{name}`, `backup-group-header-{group}` with
//! `__ungrouped` for `''`, `backup-option-{id}`,
//! `backup-option-chip-{id}`), and everything else takes an
//! internal `backup-*` key. `WindowChromeSlot` has no testid in
//! React; its title bar is `backup-window-chrome` /
//! `backup-window-title`.
//!
//! | part | key |
//! |---|---|
//! | root | `backup-window` (internal) |
//! | chrome | `backup-window-chrome` (internal) |
//! | chrome title | `backup-window-title` (internal) |
//! | file-name row | `backup-file-name-row` (internal) |
//! | file-name field | `backup-file-name` |
//! | file-name hint | `backup-file-name-hint` (internal) |
//! | column 1 | `backup-connections` (internal) |
//! | connection search | `backup-search-connection` |
//! | group header | `backup-group-header-{group or __ungrouped}` |
//! | connection row | `backup-connection-row-{id}` |
//! | column 2 | `backup-databases` (internal) |
//! | database search | `backup-search-database` |
//! | database row | `backup-db-row-{name}` |
//! | database empty state | `backup-db-empty` (internal) |
//! | column 3 | `backup-options` (internal) |
//! | type/version box | `backup-db-type` (internal) |
//! | option dropdown trigger | `backup-add-option` |
//! | option dropdown panel | `backup-options-panel` (internal, layer) |
//! | option checkbox | `backup-option-{id}` |
//! | option chip | `backup-option-chip-{id}` |
//! | chip remove | `backup-option-chip-remove-{id}` |
//! | gzip checkbox | `backup-compress-gzip` |
//! | status block | `backup-status` |
//! | progress track | `backup-progress-track` (internal) |
//! | progress fill | `backup-progress-bar` |
//! | log wrap / header | `backup-progress-log-wrap` / `-header` (internal) |
//! | log copy | `backup-progress-log-copy` |
//! | log box | `backup-progress-log` |
//! | log line | `backup-progress-log-line-{i}` (internal) |
//! | log spacer (empty log) | `backup-progress-log-spacer` (internal) |
//! | start button | `backup-start-backup` / `backup-start-restore` |
//! | restore confirm | `backup-restore-confirm` (+ the `Dialog` base's `-title` / `-close` / `-body` / `-footer`) |
//!
//! ## Host contract
//!
//! The Vm never performs IPC. The builder's callbacks are the host's
//! surface; the host owns the Vm, the [`BackupWindowEdits`], the
//! [`BackupLogPump`], and the async middle, and must honour:
//!
//! * **Text edits.** The three text fields edit through
//!   [`BackupWindowEdits`] (Rune edits text through `TextEdit`, not
//!   through the Vm's `String`s). The builder overlays the edits'
//!   current text onto its render-time Vm clone, so typing shows up
//!   without a host step; but before the host *reads* a text field
//!   (the file name for `default_file_name()`, the searches for
//!   anything else) it must call
//!   [`BackupWindowEdits::sync_into_vm`]. A host-driven reset of a
//!   Vm text field (e.g. a new default file name) must call
//!   [`BackupWindowEdits::reset_from_vm`] so the field shows it.
//! * **N4, first leg — `default_database`.** When the host feeds
//!   `apply_session_connected`, it passes
//!   `preferredDatabase ?? conn.database` as `default_database`
//!   (the `preferredDatabase` is the URL `database` param the
//!   prefill handed it, if any); the Vm does not resolve it.
//! * **N4, second leg — `take_prefill`.** The host calls
//!   `take_prefill` after *every* connections load, and when it
//!   returns a target, selects that connection with the preferred
//!   database. A `None` before the load (or for an unknown id)
//!   retries on the next load.
//! * **N4, third leg — `on_connection_closed`.** The host maps a
//!   missing `datazen:connection-closed` payload to `""` before
//!   calling it; an empty id is a no-op by design.
//! * **Runs.** The host implements `on_start_backup` /
//!   `on_start_restore` per [`super::backup_run`]: compute the
//!   file name, arm the run, drive the progress listener and the
//!   invoke, finish or fail, and — for a successful restore — emit
//!   `datazen:refresh-connection` with the session id. The start
//!   button is disabled exactly when `!can_start()`.
//! * **Log copy.** `on_copy_log` copies `vm.progress_log.join("\n")`
//!   to the clipboard; the 1500 ms "copied" feedback lives with
//!   the `ProgressLog` component (T077), so this slice always shows
//!   `t('backup.copyLog')`.
//!
//! ## Documented deviations from React
//!
//! * **`DbTypeBadge` renders the letter tile, never the driver SVG.**
//!   React resolves `db.<type>` through the theme-pack icon resolver
//!   and shows the bundled SVG when one exists; Rune has no
//!   theme-pack icon loading, so the badge is always the fallback
//!   tile ([`db_type_badge`]): the driver's short label on its
//!   `iconBg` tone at 20pt. An unknown type shows `DB` on the
//!   neutral tone, like React's `getDbIcon` fallback.
//! * **The option dropdown is a kit-anchored overlay, not an
//!   absolutely-positioned div.** React renders the panel under the
//!   trigger in-flow (`absolute top-full`); Rune puts it in the
//!   overlay layer anchored to the trigger
//!   (`Placement::BottomStart`, 4pt gap) with a dismissal scrim, the
//!   same mechanism the kit `Select` uses. The panel also closes on
//!   Escape / an outside press, where React only toggles on the
//!   trigger press.
//! * **Chips do not wrap.** React's selected-option chips wrap
//!   (`flex-wrap`); Rune rows do not wrap, so the chips sit in one
//!   row (the same documented limit as the Data Sync summary row).
//! * **The indeterminate progress bar does not pulse.** React shows
//!   a `w-1/3 animate-pulse` bar while backing with no progress
//!   yet; Rune has no CSS pulse, so the bar sits static at 1/3.
//! * **The progress log is not virtualized.** React's `ProgressLog`
//!   virtualizes its rows; this slice renders every line (bounded
//!   at 1500 by the pump). The component task (T077) replaces this
//!   inline rendering with the real `ProgressLog` — virtualization,
//!   auto-scroll pinning, and the copied feedback — keeping the
//!   same keys.
//! * **The chrome is always drawn.** React's `WindowChromeSlot`
//!   renders nothing when no chrome is registered (web) and the
//!   desktop `TitleBar` when registered; the Rune window always
//!   draws its own 40pt title band (like the main app's L3
//!   titlebar), with the slot's title as the label.
//! * **The hard-drive icon is amber, not orange-400.** React paints
//!   the group header's `HardDrive` icon `text-orange-400`; the
//!   kit has no orange token, so the warning tone stands in (no new
//!   oklch literals, per the migration discipline).

// The production consumer is the Backup window host (not yet
// written); until then the component tests are the only callers, so
// the dead-code lint is silenced here rather than in a crate-wide
// allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, icon, row, text, Color, El, ScrollSpec, TextEdit, TextStyle};
use rune_kit::{popover, scrim, Button, ButtonVariant, ControlSize, Kit, Placement};

use crate::i18n::t;
use crate::icons::{
    ICON_ALERT_TRIANGLE, ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_CONNECTIONS, ICON_COPY,
    ICON_HARD_DRIVE,
};
use crate::ui::badge::BadgeTone;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

use super::backup_db_meta::db_supports_backup;
use super::backup_grouping::{group_display_label, ConnectionGroup};
use super::backup_progress::backup_progress_ratio;
use super::backup_window_state::{BackupConnection, BackupWindowVm, DatabaseInfo};

/// The `TextEdit`s behind the window's three text fields. Rune edits
/// text through `TextEdit`; the Vm keeps the `String` model, so the
/// host owns one of these next to the Vm and keeps the two in sync
/// (see the module docs: the builder overlays the edits onto its
/// render-time Vm clone, the host syncs before reading).
#[derive(Clone, Debug, Default)]
pub(crate) struct BackupWindowEdits {
    pub file_name: TextEdit,
    pub search_conn: TextEdit,
    pub search_db: TextEdit,
}

impl BackupWindowEdits {
    /// Fresh edits from the Vm's current text (`"untitled"` and two
    /// empty searches on a fresh Vm).
    pub(crate) fn new(vm: &BackupWindowVm) -> Self {
        Self {
            file_name: TextEdit::new(vm.file_name.clone()),
            search_conn: TextEdit::new(vm.search_conn.clone()),
            search_db: TextEdit::new(vm.search_db.clone()),
        }
    }

    /// The file-name field's edit binding.
    pub(crate) fn file_name_edit(&mut self) -> &mut TextEdit {
        &mut self.file_name
    }

    /// The connection-search field's edit binding.
    pub(crate) fn search_conn_edit(&mut self) -> &mut TextEdit {
        &mut self.search_conn
    }

    /// The database-search field's edit binding.
    pub(crate) fn search_db_edit(&mut self) -> &mut TextEdit {
        &mut self.search_db
    }

    /// Copy the edits' current text into the Vm. The host calls this
    /// before reading a Vm text field (see the module docs).
    pub(crate) fn sync_into_vm(&self, vm: &mut BackupWindowVm) {
        vm.file_name = self.file_name.text.clone();
        vm.search_conn = self.search_conn.text.clone();
        vm.search_db = self.search_db.text.clone();
    }

    /// Copy the Vm's current text into the edits. The host calls
    /// this after a host-driven reset of a Vm text field, so the
    /// field shows the new value instead of the last typed one.
    pub(crate) fn reset_from_vm(&mut self, vm: &BackupWindowVm) {
        self.file_name.replace_text(vm.file_name.clone());
        self.search_conn.replace_text(vm.search_conn.clone());
        self.search_db.replace_text(vm.search_db.clone());
    }
}

/// The restore confirm dialog's content, as `useConfirmDialog`
/// hands it to `ConfirmDialog` (`kind: 'warning'`, default labels).
/// `epoch` is the [`Dialog::open_epoch`] generation: the host bumps
/// it every time the dialog opens, so a reopened dialog starts at
/// the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestoreConfirm {
    pub title: String,
    pub message: String,
    pub epoch: u64,
}

/// React `getDbIcon` (`lib/databaseTypes.ts`): the driver's short
/// label and `iconBg` tone, from the shipped driver metas
/// (`crates/drivers/*/ui/meta.ts`); an unknown type falls back to
/// `DB` on the neutral tone.
pub(crate) fn db_badge_meta(db_type: &str) -> (&'static str, BadgeTone) {
    match db_type {
        "mysql" => ("My", BadgeTone::Warning),
        "mariadb" => ("Ma", BadgeTone::Warning),
        "doris" => ("Do", BadgeTone::Warning),
        "starrocks" => ("Sr", BadgeTone::Warning),
        "manticore" => ("Mc", BadgeTone::Warning),
        "ob_oracle" => ("Ob", BadgeTone::Warning),
        "postgresql" => ("Pg", BadgeTone::Accent),
        "questdb" => ("Qd", BadgeTone::Accent),
        "cloudberry" => ("Cb", BadgeTone::Accent),
        "sqlite" => ("Lt", BadgeTone::Success),
        "rqlite" => ("Rq", BadgeTone::Accent),
        "turso" => ("Tu", BadgeTone::Success),
        "sqlserver" => ("Ss", BadgeTone::Danger),
        "clickhouse" => ("Ch", BadgeTone::Warning),
        "duckdb" => ("Dk", BadgeTone::Accent),
        "redis" => ("Rd", BadgeTone::Danger),
        "mongodb" => ("Mb", BadgeTone::Success),
        "elasticsearch" => ("Es", BadgeTone::Warning),
        "hbase" => ("Hb", BadgeTone::Warning),
        "influxdb" => ("If", BadgeTone::Warning),
        "vector" => ("Vc", BadgeTone::Success),
        "victoriametrics" => ("Vm", BadgeTone::Accent),
        _ => ("DB", BadgeTone::Neutral),
    }
}

/// React `DbTypeBadge`'s letter-tile fallback (`components/
/// DbTypeBadge.tsx`): a `size` square, `rounded-lg`, the short label
/// in bold white on the tone fill. The font is 9pt at size 20
/// (`badgeFontSize`), matching the window's `size={20}` use.
pub(crate) fn db_type_badge(key: impl Into<String>, db_type: &str, size: f32, dark: bool) -> El {
    let k = kit(dark);
    let (label, tone) = db_badge_meta(db_type);
    let fill = match tone {
        BadgeTone::Neutral => k.c.text_faint,
        BadgeTone::Success => k.c.success.fg,
        BadgeTone::Warning => k.c.warning.fg,
        BadgeTone::Danger => k.c.danger.fg,
        BadgeTone::Accent => k.c.bg_accent,
    };
    row()
        .key(key.into())
        .w(size)
        .h(size)
        .center()
        .radius(8.0)
        .fill(fill)
        .child(
            text(label.to_owned())
                .font(TextStyle::new(9.0, 700.0).lh(11.0))
                .color(Color::WHITE),
        )
}

/// The Backup window. `S` is the host state that owns the Vm, the
/// edits, and the async middle; every press calls back into it.
pub(crate) struct BackupWindow<'a, S: 'static> {
    vm: BackupWindowVm,
    edits: &'a BackupWindowEdits,
    dark: bool,
    restore_confirm: Option<RestoreConfirm>,
    on_select_connection: Option<Rc<dyn Fn(&mut S, BackupConnection)>>,
    on_select_database: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_group: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_option: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_dropdown: Option<Rc<dyn Fn(&mut S)>>,
    on_close_dropdown: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_compress: Option<Rc<dyn Fn(&mut S)>>,
    on_start_backup: Option<Rc<dyn Fn(&mut S)>>,
    on_start_restore: Option<Rc<dyn Fn(&mut S)>>,
    on_copy_log: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_restore: Option<Rc<dyn Fn(&mut S)>>,
    on_cancel_restore: Option<Rc<dyn Fn(&mut S)>>,
    on_file_name_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_search_conn_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
    on_search_db_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>>,
}

fn no_op_unit<S: 'static>() -> Rc<dyn Fn(&mut S)> {
    Rc::new(|_: &mut S| {})
}

fn no_op_string<S: 'static>() -> Rc<dyn Fn(&mut S, String)> {
    Rc::new(|_: &mut S, _: String| {})
}

fn no_op_connection<S: 'static>() -> Rc<dyn Fn(&mut S, BackupConnection)> {
    Rc::new(|_: &mut S, _: BackupConnection| {})
}

impl<'a, S: 'static> BackupWindow<'a, S> {
    pub(crate) fn new(vm: BackupWindowVm, edits: &'a BackupWindowEdits) -> Self {
        Self {
            vm,
            edits,
            dark: true,
            restore_confirm: None,
            on_select_connection: None,
            on_select_database: None,
            on_toggle_group: None,
            on_toggle_option: None,
            on_toggle_dropdown: None,
            on_close_dropdown: None,
            on_toggle_compress: None,
            on_start_backup: None,
            on_start_restore: None,
            on_copy_log: None,
            on_confirm_restore: None,
            on_cancel_restore: None,
            on_file_name_edit: None,
            on_search_conn_edit: None,
            on_search_db_edit: None,
        }
    }

    /// The restore confirm dialog to render in the layer, if the
    /// host's restore flow is waiting on it.
    pub(crate) fn restore_confirm(mut self, confirm: RestoreConfirm) -> Self {
        self.restore_confirm = Some(confirm);
        self
    }

    /// React `handleSelectConnection`: the host runs
    /// `begin_select_connection`, connects, and feeds the session
    /// back (see the module docs for the `default_database` rule).
    pub(crate) fn on_select_connection(
        mut self,
        f: impl Fn(&mut S, BackupConnection) + 'static,
    ) -> Self {
        self.on_select_connection = Some(Rc::new(f));
        self
    }

    /// React `setSelectedDb`: the database row's press.
    pub(crate) fn on_select_database(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select_database = Some(Rc::new(f));
        self
    }

    /// React `toggleGroup`: the group header's press.
    pub(crate) fn on_toggle_group(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_group = Some(Rc::new(f));
        self
    }

    /// React `toggleOption`: an option checkbox's or chip's press.
    pub(crate) fn on_toggle_option(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_option = Some(Rc::new(f));
        self
    }

    /// React `setOptionDropdownOpen(!open)`: the dropdown trigger's
    /// press.
    pub(crate) fn on_toggle_dropdown(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_dropdown = Some(Rc::new(f));
        self
    }

    /// The dropdown's dismissal (scrim / Escape): closes it.
    pub(crate) fn on_close_dropdown(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_dropdown = Some(Rc::new(f));
        self
    }

    /// React `setCompressGzip`: the gzip checkbox's press.
    pub(crate) fn on_toggle_compress(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_compress = Some(Rc::new(f));
        self
    }

    /// React `handleBackup`: the start button's press in backup
    /// mode (see [`super::backup_run`]).
    pub(crate) fn on_start_backup(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_start_backup = Some(Rc::new(f));
        self
    }

    /// React `handleRestore`: the start button's press in restore
    /// mode (see [`super::backup_run`]).
    pub(crate) fn on_start_restore(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_start_restore = Some(Rc::new(f));
        self
    }

    /// The log copy button's press: the host copies the joined log.
    pub(crate) fn on_copy_log(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_log = Some(Rc::new(f));
        self
    }

    /// The restore confirm dialog's confirm button: the host
    /// resumes the restore with the `overwrite` option.
    pub(crate) fn on_confirm_restore(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_restore = Some(Rc::new(f));
        self
    }

    /// The restore confirm dialog's cancel / close / Escape: the
    /// host aborts the restore before executing.
    pub(crate) fn on_cancel_restore(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel_restore = Some(Rc::new(f));
        self
    }

    /// The file-name field's edit binding. Unlike the action
    /// callbacks (which default to no-ops), this one must be wired:
    /// a `TextField` without `on_edit` has no edit binding, so it
    /// cannot take focus (the kit debug-asserts in `build`).
    pub(crate) fn on_file_name_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_file_name_edit = Some(Rc::new(f));
        self
    }

    /// The connection-search field's edit binding. Must be wired,
    /// like [`Self::on_file_name_edit`]: an unwired field cannot
    /// take focus.
    pub(crate) fn on_search_conn_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_search_conn_edit = Some(Rc::new(f));
        self
    }

    /// The database-search field's edit binding. Must be wired,
    /// like [`Self::on_file_name_edit`]: an unwired field cannot
    /// take focus.
    pub(crate) fn on_search_db_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_search_db_edit = Some(Rc::new(f));
        self
    }

    /// The window on its own, exactly as the probes mount it.
    pub(crate) fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same window without its kit root: the window element plus
    /// the overlay layer (option dropdown, restore confirm dialog).
    /// The Backup window host calls this once and roots the whole
    /// window once.
    pub(crate) fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        let vm = self.view_vm();
        let mut layer: Vec<El> = Vec::new();
        let mut root = col().key("backup-window").w_full().h_full().min_h(0.0);
        root = root.child(self.chrome(&vm, k));
        if !vm.is_restore {
            root = root.child(self.file_name_row(k));
        }
        let mut body = row()
            .key("backup-window-body")
            .w_full()
            .grow(1.0)
            .min_h(0.0);
        body = body.child(self.connections_column(&vm, k));
        body = body.child(self.databases_column(&vm, k));
        body = body.child(self.options_column(&vm, k, &mut layer));
        root = root.child(body);
        self.restore_confirm_dialog(k, &mut layer);
        (root, layer)
    }

    /// The Vm this frame renders: the host's Vm with the edits'
    /// current text overlaid, so typing shows up without a host
    /// sync step (see the module docs).
    fn view_vm(&self) -> BackupWindowVm {
        let mut vm = self.vm.clone();
        vm.file_name = self.edits.file_name.text.clone();
        vm.search_conn = self.edits.search_conn.text.clone();
        vm.search_db = self.edits.search_db.text.clone();
        vm
    }

    /// React `<WindowChromeSlot title={…} />`: the 40pt title band
    /// (the main app's L3 titlebar shape), with the slot's title.
    fn chrome(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let title = if vm.is_restore {
            t("common.restoreDatabase")
        } else {
            t("common.backupDatabase")
        };
        row()
            .key("backup-window-chrome")
            .window_drag()
            .w_full()
            .h(40.0)
            .center_y()
            .px(12.0)
            .child(
                text(title.to_owned())
                    .key("backup-window-title")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    }

    /// The file-name row, backup mode only: the `backup.fileName`
    /// label, the 192pt field, and the hint.
    fn file_name_row(&self, k: &Kit) -> El {
        let mut field = TextField::new("backup-file-name", &self.edits.file_name)
            .height(28.0)
            .width(192.0);
        if let Some(edit) = &self.on_file_name_edit {
            let edit = edit.clone();
            field = field.on_edit(move |s: &mut S| edit(s));
        }
        row()
            .key("backup-file-name-row")
            .w_full()
            .px(16.0)
            .py(8.0)
            .gap(12.0)
            .center_y()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .child(
                text(format!("{}:", t("backup.fileName")))
                    .key("backup-file-name-label")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base),
            )
            .child(field.build(self.dark))
            .child(
                text(t("backup.fileNameHint").to_owned())
                    .key("backup-file-name-hint")
                    .font(k.ty.small)
                    .color(k.c.text_faint),
            )
    }

    /// Column 1 (200pt): the connection search and the grouped list.
    fn connections_column(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let mut search = TextField::new("backup-search-connection", &self.edits.search_conn)
            .height(28.0)
            .placeholder(t("backup.searchConnection").to_owned());
        if let Some(edit) = &self.on_search_conn_edit {
            let edit = edit.clone();
            search = search.on_edit(move |s: &mut S| edit(s));
        }
        let mut list = col()
            .key("backup-connection-list")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            });
        for group in vm.grouped() {
            list = list.child(self.group_block(&group, vm, k));
        }
        col()
            .key("backup-connections")
            .w(200.0)
            .min_h(0.0)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                row()
                    .key("backup-search-connection-row")
                    .w_full()
                    .padding(8.0)
                    .child(search.build(self.dark).grow(1.0).min_w(0.0)),
            )
            .child(list)
    }

    /// One `grouped` row: the header (chevron + hard-drive icon +
    /// label) and, when expanded, its connection rows.
    fn group_block(&self, group: &ConnectionGroup, vm: &BackupWindowVm, k: &Kit) -> El {
        let expanded = vm.expanded_groups.contains(&group.group);
        let key = if group.group.is_empty() {
            "__ungrouped".to_owned()
        } else {
            group.group.clone()
        };
        let chevron = if expanded {
            &ICON_CHEVRON_DOWN
        } else {
            &ICON_CHEVRON_RIGHT
        };
        let toggle = self
            .on_toggle_group
            .clone()
            .unwrap_or_else(no_op_string::<S>);
        let group_key = group.group.clone();
        let mut block = col().key(format!("backup-group-{key}")).w_full();
        let header = row()
            .key(format!("backup-group-header-{key}"))
            .w_full()
            .px(8.0)
            .py(6.0)
            .gap(4.0)
            .center_y()
            .hover(k.c.hover)
            .on_press(move |s: &mut S| toggle(s, group_key.clone()))
            .child(icon(chevron, 12.0).color(k.c.text_faint))
            .child(icon(&ICON_HARD_DRIVE, 14.0).color(k.c.warning.fg))
            .child(
                text(group_display_label(&group.group))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            );
        block = block.child(header);
        if expanded {
            for conn in &group.connections {
                block = block.child(self.connection_row(conn, vm, k));
            }
        }
        block
    }

    /// One connection row: the type badge, the name (with the
    /// `(database)` suffix when one is set), and the `host :
    /// database` address. Selected rows take the accent wash and
    /// accent name; types without backup support render at 50%
    /// opacity.
    fn connection_row(&self, conn: &BackupConnection, vm: &BackupWindowVm, k: &Kit) -> El {
        let selected = vm.selected_conn_id.as_deref() == Some(conn.id.as_str());
        let can_backup = db_supports_backup(&conn.database_type);
        let select = self
            .on_select_connection
            .clone()
            .unwrap_or_else(no_op_connection::<S>);
        let pressed = conn.clone();
        let name = match conn.database.as_deref() {
            Some(db) if !db.is_empty() => format!("{} ({db})", conn.name),
            _ => conn.name.clone(),
        };
        let addr = format!(
            "{} : {}",
            conn.host.as_deref().unwrap_or("localhost"),
            conn.database.as_deref().unwrap_or("")
        );
        let mut row_el = row()
            .key(format!("backup-connection-row-{}", conn.id))
            .w_full()
            .py(6.0)
            .pl(28.0)
            .pr(8.0)
            .gap(8.0)
            .center_y()
            .hover(k.c.hover)
            .on_press(move |s: &mut S| select(s, pressed.clone()));
        if selected {
            row_el = row_el.fill(k.alpha(k.c.bg_accent, 0.2));
        }
        if !can_backup {
            row_el = row_el.opacity(0.5);
        }
        row_el
            .child(db_type_badge(
                format!("backup-connection-badge-{}", conn.id),
                &conn.database_type,
                20.0,
                self.dark,
            ))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .child(
                        text(name)
                            .font(k.ty.text_xsmall_medium)
                            .color(if selected {
                                k.c.text_accent
                            } else {
                                k.c.text_base
                            })
                            .truncate(),
                    )
                    .child(
                        text(addr)
                            .font(TextStyle::new(10.0, 400.0).lh(14.0))
                            .color(k.c.text_faint)
                            .truncate(),
                    ),
            )
    }

    /// Column 2 (200pt): the database search and the filtered list,
    /// with the empty states React renders when nothing is loaded or
    /// nothing is selected.
    fn databases_column(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let mut search = TextField::new("backup-search-database", &self.edits.search_db)
            .height(28.0)
            .placeholder(t("backup.searchDatabase").to_owned());
        if let Some(edit) = &self.on_search_db_edit {
            let edit = edit.clone();
            search = search.on_edit(move |s: &mut S| edit(s));
        }
        let mut list = col()
            .key("backup-database-list")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            });
        for db in vm.filtered_dbs() {
            list = list.child(self.database_row(db, vm, k));
        }
        if vm.databases.is_empty() && vm.selected_conn_id.is_some() {
            let hint = if vm.status_message.is_empty() {
                t("backup.selectConnectionFirst").to_owned()
            } else {
                vm.status_message.clone()
            };
            list = list.child(
                row()
                    .key("backup-db-empty")
                    .w_full()
                    .padding(12.0)
                    .justify_center()
                    .child(text(hint).font(k.ty.text_xsmall).color(k.c.text_faint)),
            );
        }
        if vm.selected_conn_id.is_none() {
            list = list.child(
                row()
                    .key("backup-db-empty")
                    .w_full()
                    .padding(12.0)
                    .justify_center()
                    .child(
                        text(t("backup.selectConnectionFirst").to_owned())
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_faint),
                    ),
            );
        }
        col()
            .key("backup-databases")
            .w(200.0)
            .min_h(0.0)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(
                row()
                    .key("backup-search-database-row")
                    .w_full()
                    .padding(8.0)
                    .child(search.build(self.dark).grow(1.0).min_w(0.0)),
            )
            .child(list)
    }

    /// One database row: the database icon and the name; the selected
    /// database takes the accent wash and accent name.
    fn database_row(&self, db: &DatabaseInfo, vm: &BackupWindowVm, k: &Kit) -> El {
        let selected = vm.selected_db.as_deref() == Some(db.name.as_str());
        let select = self
            .on_select_database
            .clone()
            .unwrap_or_else(no_op_string::<S>);
        let name = db.name.clone();
        let mut row_el = row()
            .key(format!("backup-db-row-{}", db.name))
            .w_full()
            .px(12.0)
            .py(6.0)
            .gap(8.0)
            .center_y()
            .hover(k.c.hover)
            .on_press(move |s: &mut S| select(s, name.clone()));
        if selected {
            row_el = row_el.fill(k.alpha(k.c.bg_accent, 0.2));
        }
        row_el
            .child(icon(&ICON_CONNECTIONS, 14.0).color(k.c.text_faint))
            .child(
                text(db.name.clone())
                    .font(k.ty.text_xsmall)
                    .color(if selected {
                        k.c.text_accent
                    } else {
                        k.c.text_base
                    })
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    }

    /// Column 3 (flex): the type/version box, the option controls
    /// (backup mode only), the status block, the progress log, and
    /// the start button.
    fn options_column(&self, vm: &BackupWindowVm, k: &Kit, layer: &mut Vec<El>) -> El {
        let mut column = col()
            .key("backup-options")
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .padding(12.0);
        if vm.selected_conn().is_some() {
            column = column.child(
                row()
                    .key("backup-db-type")
                    .w_full()
                    .mb(12.0)
                    .px(12.0)
                    .py(8.0)
                    .radius(6.0)
                    .fill(k.c.layer_01)
                    .border(1.0, k.c.border_base)
                    .child(
                        text(vm.db_type_display_label())
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
            if !vm.is_restore {
                column = column.child(self.option_dropdown_trigger(k));
                self.option_dropdown_panel(vm, k, layer);
                if !vm.enabled_options.is_empty() {
                    column = column.child(self.option_chips(vm, k));
                }
                column = column.child(self.compress_checkbox(vm, k));
            }
        }
        if !vm.status_message.is_empty() {
            column = column.child(self.status_block(vm, k));
        }
        column = column.child(self.progress_log_area(vm, k));
        column = column.child(self.start_button(vm, k));
        column
    }

    /// The "add option" trigger: the `backup.addOption` label and a
    /// chevron, full width, 32pt.
    fn option_dropdown_trigger(&self, k: &Kit) -> El {
        let toggle = self
            .on_toggle_dropdown
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        row()
            .key("backup-add-option")
            .w_full()
            .h(32.0)
            .mb(12.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .radius(6.0)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .on_press(move |s: &mut S| toggle(s))
            .child(
                text(t("backup.addOption").to_owned())
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_base)
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(icon(&ICON_CHEVRON_DOWN, 12.0).color(k.c.text_faint))
    }

    /// The open dropdown panel, anchored under the trigger in the
    /// overlay layer with a dismissal scrim: one checkbox per
    /// dialect option, in dialect order. Closed when
    /// `option_dropdown_open` is false.
    fn option_dropdown_panel(&self, vm: &BackupWindowVm, k: &Kit, layer: &mut Vec<El>) {
        if !vm.option_dropdown_open {
            return;
        }
        let close = self
            .on_close_dropdown
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        let close_for_scrim = close.clone();
        layer.push(scrim("backup-options-scrim", move |s: &mut S| {
            close_for_scrim(s)
        }));
        let mut panel = col()
            .key("backup-options-panel")
            .min_w(200.0)
            .padding(4.0)
            .modal(true, move |s: &mut S| close(s));
        for opt in vm.backup_options() {
            let checked = vm.enabled_options.contains(opt.id);
            let toggle = self
                .on_toggle_option
                .clone()
                .unwrap_or_else(no_op_string::<S>);
            let id = opt.id.to_owned();
            panel = panel.child(
                Checkbox::new(format!("backup-option-{}", opt.id), checked)
                    .label(opt.label)
                    .on_toggle(move |s: &mut S| toggle(s, id.clone()))
                    .build(k),
            );
        }
        layer.push(popover::anchored(
            k,
            popover::raised(k, panel, k.radius.lg),
            "backup-add-option",
            Placement::BottomStart,
            4.0,
        ));
    }

    /// The selected-option chips, in dialect order
    /// (`backup_options` filtered by `enabled_options` — the S1
    /// deviation: React shows toggle-insertion order, the Rust sets
    /// iterate sorted). Each chip carries its label and a remove
    /// press.
    fn option_chips(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let mut chips = row().key("backup-option-chips").w_full().mb(12.0).gap(4.0);
        for opt in vm.backup_options() {
            if !vm.enabled_options.contains(opt.id) {
                continue;
            }
            let toggle = self
                .on_toggle_option
                .clone()
                .unwrap_or_else(no_op_string::<S>);
            let id = opt.id.to_owned();
            let remove_id = id.clone();
            chips = chips.child(
                row()
                    .key(format!("backup-option-chip-{id}"))
                    .px(8.0)
                    .py(2.0)
                    .gap(4.0)
                    .center_y()
                    .radius(4.0)
                    .fill(k.alpha(k.c.bg_accent, 0.2))
                    .child(
                        text(opt.label.to_owned())
                            .font(k.ty.small_medium)
                            .color(k.c.text_accent),
                    )
                    .child(
                        row()
                            .key(format!("backup-option-chip-remove-{id}"))
                            .label(format!("Remove {}", opt.label))
                            .on_press(move |s: &mut S| toggle(s, remove_id.clone()))
                            .child(
                                text("×".to_owned())
                                    .font(k.ty.small_medium)
                                    .color(k.c.text_accent),
                            ),
                    ),
            );
        }
        chips
    }

    /// The gzip checkbox (`backup.compressGzip`).
    fn compress_checkbox(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let toggle = self
            .on_toggle_compress
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        Checkbox::new("backup-compress-gzip", vm.compress_gzip)
            .label(t("backup.compressGzip"))
            .on_toggle(move |s: &mut S| toggle(s))
            .build(k)
            .mb(16.0)
    }

    /// The status block: the status line (fixed 20pt, truncated)
    /// and, while a run is backing or a progress payload is set,
    /// the progress bar under it.
    fn status_block(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let mut block = col()
            .key("backup-status")
            .w_full()
            .mb(12.0)
            .px(12.0)
            .py(8.0)
            .radius(6.0)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .child(
                text(vm.status_message.clone())
                    .key("backup-status-text")
                    .font(k.ty.text_xsmall.lh(20.0))
                    .color(k.c.text_muted)
                    .truncate(),
            );
        if vm.backing || vm.progress.is_some() {
            block = block.child(self.progress_bar(vm, k));
        }
        block
    }

    /// The progress bar: a 6pt track with an accent fill whose
    /// share is [`backup_progress_ratio`]; while backing with no
    /// payload yet the fill sits static at 1/3 (see the module
    /// docs: no pulse in Rune).
    fn progress_bar(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let ratio = match &vm.progress {
            Some(payload) => backup_progress_ratio(payload),
            None => 1.0 / 3.0,
        };
        let ratio = ratio as f32;
        row()
            .key("backup-progress-track")
            .w_full()
            .h(6.0)
            .mt(8.0)
            .radius(3.0)
            .fill(k.c.border_base)
            .child(
                row()
                    .key("backup-progress-bar")
                    .h_full()
                    .grow(ratio)
                    .radius(3.0)
                    .fill(k.c.bg_accent),
            )
            .child(col().grow(1.0 - ratio))
    }

    /// The progress log: the real lines where S1 left the empty
    /// [`super::backup_window_state::progress_log_mount`], or the
    /// `flex-1` spacer while the log is empty. T077 replaces this
    /// inline rendering with the `ProgressLog` component (see the
    /// module docs).
    fn progress_log_area(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        if vm.progress_log.is_empty() {
            return col().key("backup-progress-log-spacer").grow(1.0).min_h(0.0);
        }
        let copy = self.on_copy_log.clone().unwrap_or_else(no_op_unit::<S>);
        let mut lines = col()
            .key("backup-progress-log")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .scroll(ScrollSpec { stick_bottom: true });
        for (index, line) in vm.progress_log.iter().enumerate() {
            lines = lines.child(
                text(line.clone())
                    .key(format!("backup-progress-log-line-{index}"))
                    .font(TextStyle::new(11.0, 400.0).mono().lh(20.0))
                    .color(k.c.text_muted)
                    .truncate(),
            );
        }
        col()
            .key("backup-progress-log-wrap")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .mb(12.0)
            .child(
                row()
                    .key("backup-progress-log-header")
                    .w_full()
                    .mb(4.0)
                    .center_y()
                    .child(
                        text(t("backup.progressLog").to_owned())
                            .font(k.ty.small_medium)
                            .color(k.c.text_faint)
                            .grow(1.0)
                            .min_w(0.0),
                    )
                    .child(
                        row()
                            .key("backup-progress-log-copy")
                            .px(6.0)
                            .py(2.0)
                            .gap(4.0)
                            .center_y()
                            .radius(4.0)
                            .hover(k.c.hover)
                            .on_press(move |s: &mut S| copy(s))
                            .child(icon(&ICON_COPY, 12.0).color(k.c.text_faint))
                            .child(
                                text(t("backup.copyLog").to_owned())
                                    .font(k.ty.small)
                                    .color(k.c.text_faint),
                            ),
                    ),
            )
            .child(lines)
    }

    /// The start button: disabled exactly when `!can_start()`, and
    /// labelled for the run state (in-progress / start, backup /
    /// restore).
    fn start_button(&self, vm: &BackupWindowVm, k: &Kit) -> El {
        let label = if vm.backing {
            if vm.is_restore {
                t("backup.restoring")
            } else {
                t("backup.inProgress")
            }
        } else if vm.is_restore {
            t("backup.startRestore")
        } else {
            t("backup.startBackup")
        };
        let key = if vm.is_restore {
            "backup-start-restore"
        } else {
            "backup-start-backup"
        };
        let start = if vm.is_restore {
            self.on_start_restore
                .clone()
                .unwrap_or_else(no_op_unit::<S>)
        } else {
            self.on_start_backup.clone().unwrap_or_else(no_op_unit::<S>)
        };
        let button = Button::new(label)
            .variant(ButtonVariant::Submit)
            .disabled(!vm.can_start())
            .on_press(move |s: &mut S| start(s))
            .build(k)
            .key(key);
        row()
            .key("backup-start-row")
            .w_full()
            .justify_end()
            .child(button)
    }

    /// The restore confirm dialog (`useConfirmDialog`'s element, in
    /// the layer): the warning title + message, a ghost cancel and a
    /// danger confirm (`kind: 'warning'`), Escape / close / scrim
    /// cancelling. Mounts nothing when no confirm is pending.
    fn restore_confirm_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let Some(confirm) = &self.restore_confirm else {
            return;
        };
        let cancel = self
            .on_cancel_restore
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        let ok = self
            .on_confirm_restore
            .clone()
            .unwrap_or_else(no_op_unit::<S>);
        let cancel_for_button = cancel.clone();
        let cancel_button = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .on_press(move |s: &mut S| cancel_for_button(s))
            .build(k)
            .key("confirm-dialog-cancel");
        let ok_button = Button::new(t("common.confirm"))
            .variant(ButtonVariant::Danger)
            .control_size(ControlSize::Small)
            .on_press(move |s: &mut S| ok(s))
            .build(k)
            .key("confirm-dialog-ok");
        Dialog::new("backup-restore-confirm", confirm.title.clone())
            .open(true)
            .open_epoch(confirm.epoch)
            .fit()
            .width(512.0)
            .child(
                row()
                    .key("backup-restore-confirm-body")
                    .w_full()
                    .gap(12.0)
                    .child(icon(&ICON_ALERT_TRIANGLE, 20.0).color(k.c.warning.fg))
                    .child(
                        text(confirm.message.clone())
                            .font(k.ty.body)
                            .color(k.c.text_muted)
                            .grow(1.0)
                            .min_w(0.0),
                    ),
            )
            .footer(cancel_button)
            .footer(ok_button)
            .on_close(move |s: &mut S| cancel(s))
            .build(k, layer);
    }
}
