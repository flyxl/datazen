//! `ProcessListView`, ported 1:1 from
//! `packages/frontend/src/windows/connection/ProcessListView.tsx` with the pure
//! column helper from `lib/processListResult.ts` translated below.
//!
//! React is a panel with a 36pt toolbar (title, connection name, refresh and
//! kill buttons), an error banner and a `DataTable` of process rows. Loading
//! and kill are IPC commands; this port owns neither — it emits
//! [`ProcessListAction`]s through an injected host port and renders whatever
//! [`ProcessListViewVm`] snapshot the host feeds back. No fake IPC, promise or
//! thread. The kill *confirmation* is host-owned in React too (`useConfirmDialog`).
//!
//! ## Documented deviations
//!
//! * `rune_kit` has no Skull glyph; the kill button uses the registry `ICON_X`
//!   (terminate). Refresh uses the exact `ICON_REFRESH` counterpart.
//! * The Rune `DataTable` base takes sort as an input but does not make its
//!   header cells pressable. A minimal keyed sort strip carries the same exact
//!   localized column titles and emits `Sort`; remove it when the table base
//!   gains an `on_sort` port.
//! * React's kill button executes `kill_process` with `force: true` after the
//!   confirm; the host owns that command and the refresh that follows.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this view yet. The connection-page host will own
//! the VM and translate the four actions into its driver commands; until then
//! every reusable item carries a per-item `#[allow(dead_code)]` with this
//! removal stage.

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, SortDir};

use crate::i18n::t;
use crate::icons::{ICON_LOADER2, ICON_REFRESH, ICON_X};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::table::{CellValue, Column, DataTable, Row};

const ROOT: &str = "process-list-view";
/// React's `h-9` toolbar.
const TOOLBAR_HEIGHT: f32 = 36.0;

/// React `ProcessListCache.columns` / `ColumnInfo` projected into the view.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProcessColumn {
    pub(crate) name: String,
    pub(crate) data_type: String,
}

/// React `QueryResult` reduced to what the table paints.
#[allow(dead_code)] // VM mirror; supplied by the host result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProcessResult {
    pub(crate) columns: Vec<ProcessColumn>,
    pub(crate) rows: Vec<Vec<String>>,
}

/// React's `PROCESS_COLUMN_LABELS` map (`lib/processListResult.ts`).
#[allow(dead_code)] // pure mapping; consumed by the column builder and tests.
pub(crate) fn column_label_key(name: &str) -> Option<&'static str> {
    match name {
        "pid" => Some("processList.colPid"),
        "user" => Some("processList.colUser"),
        "database" => Some("processList.colDatabase"),
        "client" => Some("processList.colClient"),
        "state" => Some("processList.colState"),
        "query" => Some("processList.colQuery"),
        "durationMs" => Some("processList.colDuration"),
        _ => None,
    }
}

/// React `result.columns.findIndex(c => c.name.toLowerCase() === 'pid')`.
#[allow(dead_code)] // pure helper; consumed by kill gating and tests.
pub(crate) fn pid_column_index(result: &ProcessResult) -> Option<usize> {
    result
        .columns
        .iter()
        .position(|column| column.name.to_ascii_lowercase() == "pid")
}

impl ProcessResult {
    /// React `commandResultColumns(result.columns, t)` with the default set as
    /// fallback when the host supplied no columns.
    #[allow(dead_code)] // pure helper; consumed by the table builder and tests.
    pub(crate) fn columns(&self) -> Vec<Column> {
        let source: Vec<ProcessColumn> = if self.columns.is_empty() {
            ["pid", "user", "database", "client", "state", "query", "durationMs"]
                .into_iter()
                .map(|name| ProcessColumn {
                    name: name.to_owned(),
                    data_type: if name == "pid" || name == "durationMs" { "integer" } else { "string" }.to_owned(),
                })
                .collect()
        } else {
            self.columns.clone()
        };
        source
            .iter()
            .map(|column| {
                let title = column_label_key(&column.name).map(t).unwrap_or(&column.name).to_owned();
                let col = Column::new(title);
                if column.name == "pid" || column.name == "durationMs" {
                    col.width(72.0).end()
                } else if column.name == "query" {
                    col
                } else {
                    col.width(120.0)
                }
            })
            .collect()
    }

    /// React `rows` as table cells; a null cell is shown as an em dash.
    #[allow(dead_code)] // pure helper; consumed by the table builder and tests.
    pub(crate) fn table_rows(&self) -> Vec<Row> {
        self.rows
            .iter()
            .enumerate()
            .map(|(index, values)| {
                let key = format!("process-row-{index}");
                let cells = values
                    .iter()
                    .map(|value| {
                        let cell = CellValue::new(if value.is_empty() { "—".to_owned() } else { value.clone() });
                        if value.parse::<f64>().is_ok() {
                            cell.mono()
                        } else {
                            cell.muted()
                        }
                    })
                    .collect();
                Row { key: Some(key), cells }
            })
            .collect()
    }
}

/// The full view snapshot the host fills each frame.
#[allow(dead_code)] // VM surface; owned by the connection-page host.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessListViewVm {
    pub(crate) db_session_id: String,
    pub(crate) connection_name: String,
    pub(crate) result: Option<ProcessResult>,
    pub(crate) loading: bool,
    pub(crate) error: String,
    /// Rendered-order row index, matching React's `onRowClick(row)`.
    pub(crate) highlighted_row: Option<usize>,
    pub(crate) killing: bool,
    pub(crate) sort: Option<(usize, SortDir)>,
}

impl ProcessListViewVm {
    /// React's kill gate: a parsed row, a selected row and a pid column.
    #[allow(dead_code)] // pure helper; consumed by the kill button and tests.
    pub(crate) fn can_kill(&self) -> bool {
        !self.killing && self.highlighted_row.is_some() && self.pid_index().is_some()
    }

    fn pid_index(&self) -> Option<usize> {
        self.result.as_ref().and_then(pid_column_index)
    }

    /// The pid of the highlighted row, after mapping rendered → base order.
    #[allow(dead_code)] // pure helper; consumed by the kill action and tests.
    pub(crate) fn highlighted_pid(&self) -> Option<i64> {
        let result = self.result.as_ref()?;
        let column = self.pid_index()?;
        let rendered = self.highlighted_row?;
        let base = rendered_order(result, self.sort).get(rendered).copied()?;
        result.rows.get(base)?.get(column)?.trim().parse().ok()
    }
}

/// The rendered row order the table base computes, mirrored so selection maps
/// back to the host's base-order rows.
#[allow(dead_code)] // pure helper; consumed by selection and tests.
pub(crate) fn rendered_order(result: &ProcessResult, sort: Option<(usize, SortDir)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..result.rows.len()).collect();
    if let Some((column, dir)) = sort {
        order.sort_by(|&a, &b| {
            let left = result.rows.get(a).and_then(|row| row.get(column)).map(String::as_str).unwrap_or("");
            let right = result.rows.get(b).and_then(|row| row.get(column)).map(String::as_str).unwrap_or("");
            let cmp = match (left.trim().parse::<f64>(), right.trim().parse::<f64>()) {
                (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                _ => left.to_lowercase().cmp(&right.to_lowercase()).then_with(|| left.cmp(right)),
            };
            dir.apply(cmp)
        });
    }
    order
}

/// Intents the host applies; the kill action carries React's confirm copy.
#[allow(dead_code)] // host port; remove when the connection-page host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProcessListAction {
    Refresh,
    KillRequest {
        db_session_id: String,
        pid: i64,
        title: String,
        message: String,
        confirm_label: String,
    },
    SelectRow(usize),
    Sort(usize),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ProcessListAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: ProcessListAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the panel.
#[allow(dead_code)] // consumer: the connection-page host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &ProcessListViewVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let result = vm.result.clone().unwrap_or_default();
    let columns = result.columns();
    let rows = result.table_rows();

    let refresh = icon_button(
        &ICON_REFRESH,
        "process-list-refresh",
        t("processList.refresh"),
        vm.loading,
        &on_action,
        ProcessListAction::Refresh,
        dark,
    );
    let kill_target = [vm.connection_name.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let kill_target = match vm.highlighted_pid() {
        Some(pid) => {
            let part = format!("{}: {}", t("processList.colPid"), pid);
            if kill_target.is_empty() { part } else { format!("{kill_target} · {part}") }
        }
        None => kill_target,
    };
    let kill_message = match vm.highlighted_pid() {
        Some(pid) => format!(
            "{}\n{}",
            t("processList.killConfirm").replace("{pid}", &pid.to_string()),
            kill_target
        ),
        None => t("processList.killTitle").to_owned(),
    };
    let kill_action = vm.highlighted_pid().map(|pid| ProcessListAction::KillRequest {
        db_session_id: vm.db_session_id.clone(),
        pid,
        title: t("processList.killTitle").to_owned(),
        message: kill_message,
        confirm_label: t("processList.kill").to_owned(),
    });
    let mut kill = Button::new(t("processList.kill"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(if vm.killing { &ICON_LOADER2 } else { &ICON_X })
        .disabled(!vm.can_kill());
    if let (Some(callback), Some(action)) = (on_action.clone(), kill_action) {
        kill = kill.on_press(move |s: &mut S| callback(s, action.clone()));
    }
    let kill = kill.build(k).key("process-list-kill");

    let toolbar = row()
        .key("process-list-toolbar")
        .w_full()
        .h(TOOLBAR_HEIGHT)
        .shrink()
        .px(12.0)
        .gap(8.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(text(t("common.processList")).font(TextStyle::new(13.0, 600.0)).color(k.c.text_base))
        .child(
            text(vm.connection_name.clone())
                .key("process-list-connection")
                .color(k.c.text_muted)
                .truncate()
                .shrink()
                .help(vm.connection_name.clone()),
        )
        .child(row().w(1.0).h(12.0).fill(k.c.border_base))
        .child(row().grow(1.0))
        .child(refresh)
        .child(kill);

    let mut body = col().key("process-list-body").w_full().min_w(0.0).grow(1.0).min_h(0.0).gap(6.0);
    if !vm.error.is_empty() {
        body = body.child(CopyableError::new("process-list-error", vm.error.clone()).build::<S>(k));
    }
    if !columns.is_empty() {
        let mut sort_strip = row().key("process-sort-strip").w_full().gap(6.0).flex_wrap().center_y();
        for (index, column) in columns.iter().enumerate() {
            sort_strip = sort_strip.child(
                text(column.title.clone())
                    .key(format!("process-sort-{index}"))
                    .role("button")
                    .focusable(true)
                    .color(if vm.sort.is_some_and(|(col, _)| col == index) { k.c.text_base } else { k.c.text_muted })
                    .on_press(emit(&on_action, ProcessListAction::Sort(index))),
            );
        }
        body = body.child(sort_strip);
    }

    let mut table = DataTable::new("process-list-table", &columns)
        .rows(rows)
        .grow()
        .label(t("common.processList"));
    if let Some(sort) = vm.sort {
        table = table.sort(sort.0, sort.1);
    }
    table = table.selected(vm.highlighted_row);
    if let Some(callback) = on_action {
        table = table.on_select(move |s: &mut S, row| callback(s, ProcessListAction::SelectRow(row)));
    }
    body = body.child(table.build(dark));

    col()
        .key(ROOT)
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(toolbar)
        .child(body)
}

fn icon_button<S: 'static>(
    glyph: &'static rune::Icon,
    key: &str,
    label: &str,
    disabled: bool,
    on_action: &Option<ActionFn<S>>,
    action: ProcessListAction,
    dark: bool,
) -> El {
    let mut button = Button::new("")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(if disabled { &ICON_LOADER2 } else { glyph })
        .help(label)
        .disabled(disabled);
    if !disabled {
        button = button.on_press(emit(on_action, action));
    }
    button.build(kit(dark)).key(key).label(label)
}
