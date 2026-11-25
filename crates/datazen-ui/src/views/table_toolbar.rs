//! Port of `packages/frontend/src/windows/connection/TableToolbar.tsx`.
//!
//! React is the panel toolbar: two table-level buttons (refresh, insert row)
//! rendered as the leading cluster of `TableFilterToolbar`, which owns the
//! filter half. Every `data-testid` here is an E2E anchor, kept verbatim. The
//! page reload and new-row draft are host commands; this port renders the
//! buttons and emits intents through an injected host port — no fake IPC,
//! promise or timer lives here. The filter half is delegated to the existing
//! [`table_filter_toolbar`] port with its actions threaded through the same
//! host port as [`TableToolbarAction::Filter`].
//!
//! ## Keys
//!
//! `table-data-refresh` and `table-insert-row` are React's test ids; the four
//! filter anchors (`table-filter-toggle`, `table-quick-filter`, ... ) come from
//! the delegated filter toolbar.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this toolbar yet; the table-panel host owns the
//! VM, performs the reload and row-draft writes, and maps the intents. Until
//! then every reusable item carries a per-item `#[allow(dead_code)]` with this
//! stage.

use std::rc::Rc;

use rune::{col, icon, row, El, TextEdit};

use crate::i18n::t;
use crate::icons::{ICON_LOADER2, ICON_PLUS, ICON_REFRESH};
use crate::ui::kit::kit;
use crate::views::table_filter_toolbar::{
    build as build_filter, TableFilterAction, TableFilterToolbarVm,
};

pub const REFRESH_KEY: &str = "table-data-refresh";
pub const INSERT_KEY: &str = "table-insert-row";

/// React `TableToolbarProps` plus the filter half's VM (delegated as-is).
#[allow(dead_code)] // VM surface; owned by the table-panel host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableToolbarVm {
    pub loading: bool,
    pub pending_busy: bool,
    pub is_editable: bool,
    pub filter: TableFilterToolbarVm,
}

/// Every host intent the toolbar can emit.
#[allow(dead_code)] // host port; remove when the table-panel host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableToolbarAction {
    Refresh,
    AddDraftRow,
    Filter(TableFilterAction),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, TableToolbarAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: TableToolbarAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the full toolbar: leading table buttons + the filter half.
#[allow(dead_code)] // view entry; remove when the table-panel host lands.
pub fn build<S: 'static>(
    vm: &TableToolbarVm,
    quick_filter: &TextEdit,
    quick_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut leading = row().key("table-toolbar-leading").gap(2.0).center_y();

    let refresh_help = t("connWin.refresh").to_owned();
    let refresh_action = emit(&on_action, TableToolbarAction::Refresh);
    let mut refresh = row()
        .key(REFRESH_KEY)
        .w(28.0)
        .h(28.0)
        .shrink()
        .center()
        .radius(4.0)
        .focusable(!vm.loading)
        .help(refresh_help.clone())
        .label(refresh_help)
        .child(icon(if vm.loading { &ICON_LOADER2 } else { &ICON_REFRESH }, 14.0).color(k.c.text_muted));
    if !vm.loading {
        refresh = refresh.on_press(refresh_action);
    }
    leading = leading.child(refresh);

    if vm.is_editable {
        let shortcut = t("tableData.insertRowHint").replace("{shortcut}", "⌘/Ctrl+I");
        let insert_disabled = vm.loading || vm.pending_busy;
        let mut insert = row()
            .key(INSERT_KEY)
            .w(28.0)
            .h(28.0)
            .shrink()
            .center()
            .radius(4.0)
            .focusable(!insert_disabled)
            .help(shortcut)
            .label(t("tableData.insertRow"))
            .child(icon(&ICON_PLUS, 14.0).color(k.c.text_muted));
        if !insert_disabled {
            insert = insert.on_press(emit(&on_action, TableToolbarAction::AddDraftRow));
        }
        leading = leading.child(insert);
    }

    // Thread the delegated filter actions through the same host port.
    let filter_port: Option<Rc<dyn Fn(&mut S, TableFilterAction) + 'static>> =
        on_action.clone().map(|callback| {
            Rc::new(move |s: &mut S, action: TableFilterAction| {
                callback(s, TableToolbarAction::Filter(action));
            }) as Rc<dyn Fn(&mut S, TableFilterAction) + 'static>
        });

    let filter_vm = &vm.filter;
    let mut root = col().key("table-toolbar").w_full().min_w(0.0).gap(0.0);
    root = root.child(build_filter(
        filter_vm,
        quick_filter,
        quick_edit,
        Some(leading),
        None,
        filter_port,
        dark,
    ));
    root
}
