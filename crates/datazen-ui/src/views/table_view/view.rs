//! Composition of the `TableView` port: status banners, read-only tip, the
//! grid adapter and the insert / Quick Look marks. All decisions come from
//! [`super::logic`]; this module only builds elements.

use std::rc::Rc;

use rune::{col, El};

use super::*;
use crate::views::table_status_banners::{
    build as build_status_banners, TableStatusBannersAction, TableStatusBannersVm,
};
use crate::views::table_read_only_tip::{
    build as build_read_only_tip, CellReadOnlyReason as TipReason, TableReadOnlyTipAction,
    TableReadOnlyTipVm,
};
use crate::ui::table::{CellValue as TableCell, Column, DataTable, Row};

/// Build the panel into one column.
#[allow(dead_code)] // consumer: the table host (not landed).
pub(crate) fn build<S: 'static>(
    vm: &TableViewVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let mut root = col().key("table-view").w_full().h_full().min_h(0.0);

    let banners = TableStatusBannersVm {
        error: vm.error.clone(),
        has_work: vm.has_work || has_pending_work(vm),
        insert_notice: vm.insert_notice.clone(),
    };
    let banner_callback: Option<Rc<dyn Fn(&mut S, TableStatusBannersAction) + 'static>> =
        on_action.as_ref().map(|callback| {
            let callback = callback.clone();
            Rc::new(move |state: &mut S, action| match action {
                TableStatusBannersAction::Retry => callback(state, TableViewAction::Retry),
            }) as Rc<dyn Fn(&mut S, TableStatusBannersAction) + 'static>
        });
    root = root.child(build_status_banners(&banners, banner_callback, dark));

    if let Some(reason) = vm.show_read_only_tip {
        let tip = TableReadOnlyTipVm {
            reason: Some(match reason {
                ReadOnlyReason::ConnectionReadOnly => TipReason::ConnectionReadOnly,
                ReadOnlyReason::DriverCapability => TipReason::DriverCapability,
                ReadOnlyReason::View => TipReason::View,
                ReadOnlyReason::NoPrimaryKey => TipReason::NoPrimaryKey,
                ReadOnlyReason::AmbiguousRowIdentity => TipReason::AmbiguousRowIdentity,
                ReadOnlyReason::AutoIncrement => TipReason::AutoIncrement,
                ReadOnlyReason::Generated => TipReason::Generated,
                ReadOnlyReason::BinaryColumn => TipReason::BinaryColumn,
                ReadOnlyReason::LargeValue => TipReason::LargeValue,
                ReadOnlyReason::TypeNotEditable => TipReason::TypeNotEditable,
            }),
            dismissible: false,
        };
        let tip_callback: Option<Rc<dyn Fn(&mut S, TableReadOnlyTipAction) + 'static>> =
            on_action.as_ref().map(|callback| {
                let callback = callback.clone();
                Rc::new(move |state: &mut S, _action| {
                    callback(state, TableViewAction::CancelEdit)
                }) as Rc<dyn Fn(&mut S, TableReadOnlyTipAction) + 'static>
            });
        root = root.child(build_read_only_tip(&tip, tip_callback, dark));
    }

    if let Some(deadline) = vm.read_only_tip_until {
        root = root.child(el_key("table-view-tip-deadline", deadline.to_string()));
    }
    if !navigation_allowed(vm) {
        root = root.child(el_key("table-view-navigation-blocked", "invalid-draft".to_owned()));
    }
    if vm.inserted_row.is_some() {
        root = root.child(el_key(
            "table-view-inserted-row",
            vm.inserted_row.map(|value| value.to_string()).unwrap_or_default(),
        ));
    }

    let names = displayed_columns(&vm.columns, vm.visible_columns.as_deref(), vm.column_order.as_deref());
    let columns: Vec<Column> = vm
        .columns
        .iter()
        .filter(|column| names.contains(&column.name))
        .map(|column| Column::new(column.name.clone()).width(160.0))
        .collect();
    let rows: Vec<Row> = vm
        .rows
        .iter()
        .enumerate()
        .map(|(index, values)| {
            let cells: Vec<TableCell> = names
                .iter()
                .filter_map(|name| vm.columns.iter().position(|column| &column.name == name))
                .filter_map(|column_index| values.get(column_index))
                .map(|value| {
                    let cell = TableCell::new(value.display());
                    if matches!(value, CellValue::Null) {
                        cell.muted()
                    } else {
                        cell.mono()
                    }
                })
                .collect();
            Row {
                key: Some(format!("table-view-row-{index}")),
                cells,
            }
        })
        .collect();

    let mut table = DataTable::new("table-view-grid", &columns)
        .rows(rows)
        .grow()
        .empty_text(crate::i18n::t("sqlFile.noResults"));
    if let Some((column, dir)) = vm.sort {
        table = table.sort(column, dir);
    }
    table = table.selected(vm.highlighted_row);
    if !vm.drafts.is_empty() {
        root = root.child(el_key("table-view-insert-draft", vm.drafts.len().to_string()));
    }
    if let Some((row, column)) = vm.quick_look.as_ref() {
        root = root.child(el_key("table-view-quick-look", format!("{row}:{column}")));
    }
    root.child(table.build(dark))
}

fn el_key(key: &str, label: String) -> El {
    rune::el().key(key).a11y_hidden().label(label)
}
