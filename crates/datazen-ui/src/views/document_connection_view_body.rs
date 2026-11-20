//! Rendering halves of the document connection view. Every branch is driven by
//! the host-owned VM from `document_connection_view.rs`; no state or IPC lives
//! in this module. Keys mirror the React structure (see the main module docs).

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_PLUS, ICON_REFRESH, ICON_TRASH};
use crate::ui::copyable::CopyableError;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::ui::table::{CellValue, Column, DataTable, Row};

use super::{DocAction, DocTab, DocumentViewVm};

type ActionFn<S> = Rc<dyn Fn(&mut S, DocAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: DocAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Compose the whole view (header + active tab body) into `layer`.
pub(crate) fn build_body<S: 'static>(
    vm: &DocumentViewVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    let mut root = col()
        .key("doc-view")
        .w_full()
        .h_full()
        .min_w(0.0)
        .min_h(0.0)
        .gap(0.0);

    // Header: refresh, two tabs and the connection name.
    let refresh = Button::new("")
        .icon(&ICON_REFRESH)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .build(k)
        .key("doc-view-refresh")
        .on_press(emit(&on_action, DocAction::Refresh));
    let tab_documents = tab_button(
        "doc-view-tab-documents",
        t("docView.documents"),
        vm.active_tab == DocTab::Documents,
        dark,
        &on_action,
        DocAction::SelectTab(DocTab::Documents),
    );
    let tab_queries = tab_button(
        "doc-view-tab-queries",
        t("docView.queries"),
        vm.active_tab == DocTab::Queries,
        dark,
        &on_action,
        DocAction::SelectTab(DocTab::Queries),
    );

    root = root.child(
        row()
            .key("doc-view-header")
            .w_full()
            .min_w(0.0)
            .h(36.0)
            .gap(8.0)
            .center_y()
            .child(refresh)
            .child(tab_documents)
            .child(tab_queries)
            .child(row().grow(1.0).min_w(0.0))
            .child(text(vm.connection_name.clone()).key("doc-view-connection-name").wrap()),
    );

    if vm.active_tab == DocTab::Documents {
        root = root.child(build_documents(vm, &on_action, dark));
    } else {
        root = root.child(build_queries(vm, &on_action, dark));
    }
    layer.push(root);
}

fn tab_button_plain(key: &str, label: &str, selected: bool, dark: bool) -> El {
    let theme = kit(dark);
    row()
        .key(key)
        .role("button")
        .focusable(true)
        .label(label)
        .h(32.0)
        .px(12.0)
        .center()
        .child(text(label).wrap())
        .child(
            row()
                .key(format!("{key}-underline"))
                .h(2.0)
                .w_full()
                .fill(if selected { theme.c.text_base } else { theme.c.border_muted }),
        )
}

fn tab_button<S: 'static>(
    key: &str,
    label: &str,
    selected: bool,
    dark: bool,
    on_action: &Option<ActionFn<S>>,
    action: DocAction,
) -> El {
    tab_button_plain(key, label, selected, dark).on_press(emit(on_action, action))
}

fn build_documents<S: 'static>(
    vm: &DocumentViewVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut root = row().key("doc-view-documents").w_full().grow(1.0).min_h(0.0).min_w(0.0);

    // Sidebar: databases, collection search and collections.
    let mut sidebar = col()
        .key("doc-view-sidebar")
        .w(224.0)
        .shrink()
        .h_full()
        .min_h(0.0)
        .gap(8.0);
    sidebar = sidebar.child(text(t("docView.databases")).key("doc-view-databases").wrap());
    if vm.loading_databases {
        sidebar = sidebar.child(loading_row("doc-view-db-loading", dark));
    }
    for db in &vm.databases {
        let action = DocAction::SelectDb(db.clone());
        sidebar = sidebar.child(
            row()
                .key(format!("doc-view-db-{db}"))
                .role("button")
                .focusable(true)
                .label(db.clone())
                .w_full()
                .px(8.0)
                .py(4.0)
                .child(text(db.clone()).selectable(true).wrap())
                .on_press(emit(on_action, action)),
        );
    }
    sidebar = sidebar.child(
        TextField::new("doc-view-collection-search", &TextEdit::new(vm.collection_filter.clone()))
            .height(28.0)
            .placeholder(t("docView.searchCollections"))
            .disabled(true)
            .build(dark),
    );
    sidebar = sidebar.child(text(t("docView.collections")).key("doc-view-collections").wrap());
    if vm.collections_loading {
        sidebar = sidebar.child(loading_row("doc-view-collections-loading", dark));
    }
    if !vm.collections_loading && vm.filtered_collections().is_empty() {
        sidebar = sidebar.child(text(t("docView.noCollections")).key("doc-view-no-collections").wrap());
    }
    for collection in vm.filtered_collections() {
        let action = DocAction::SelectCollection(collection.clone());
        sidebar = sidebar.child(
            row()
                .key(format!("doc-view-collection-{collection}"))
                .role("button")
                .focusable(true)
                .label(collection.clone())
                .w_full()
                .px(8.0)
                .py(4.0)
                .child(text(collection.clone()).selectable(true).wrap())
                .on_press(emit(on_action, action)),
        );
    }
    root = root.child(sidebar);

    // Main column.
    let mut main = col().key("doc-view-main").w_full().grow(1.0).min_h(0.0).min_w(0.0);
    if let (Some(db), Some(collection)) = (vm.selected_db.as_ref(), vm.selected_collection.as_ref()) {
        main = main.child(build_toolbar(vm, db, collection, on_action, dark));
        if !vm.filter_error.is_empty() {
            main = main.child(
                CopyableError::new("doc-view-filter-error", vm.filter_error.clone()).build::<S>(k),
            );
        }
        if !vm.docs_error.is_empty() {
            main = main.child(
                CopyableError::new("doc-view-docs-error", vm.docs_error.clone()).build::<S>(k),
            );
        }
        let mut content = row().key("doc-view-content").w_full().grow(1.0).min_h(0.0).min_w(0.0);
        content = content.child(build_results(vm, on_action, dark));
        if vm.show_editor() {
            content = content.child(build_detail_editor(vm, on_action, dark));
        }
        main = main.child(content);
    } else {
        main = main.child(
            row()
                .key("doc-view-select-collection")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(text(t("docView.selectCollection")).wrap()),
        );
    }
    root.child(main)
}

fn build_toolbar<S: 'static>(
    vm: &DocumentViewVm,
    db: &str,
    collection: &str,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let filter = TextField::new("doc-view-filter", &TextEdit::new(vm.filter_text.clone()))
        .height(28.0)
        .width(220.0)
        .shrink()
        .placeholder("{\"status\":\"paid\"}")
        .disabled(true)
        .build(dark);
    let apply = Button::new(t("docView.applyFilter"))
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .disabled(vm.docs_loading)
        .build(k)
        .key("doc-view-apply-filter")
        .on_press(emit(on_action, DocAction::ApplyFilter));
    let insert = Button::new(t("docView.insert"))
        .icon(&ICON_PLUS)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .disabled(vm.docs_loading || vm.mutating)
        .build(k)
        .key("doc-view-insert")
        .on_press(emit(on_action, DocAction::InsertNew));
    let mut toolbar = row()
        .key("doc-view-toolbar")
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .child(text(format!("{db}.{collection}")).key("doc-view-selection").wrap())
        .child(filter)
        .child(apply)
        .child(insert);
    if vm.result.is_some() {
        toolbar = toolbar.child(
            text(t("docView.docCount").replace("{count}", &vm.doc_count().to_string()))
                .key("doc-view-doc-count")
                .wrap(),
        );
    }
    toolbar
}

fn build_results<S: 'static>(
    vm: &DocumentViewVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let mut column = col().key("doc-view-results").w_full().grow(1.0).min_h(0.0).min_w(0.0);
    if vm.docs_loading {
        return column.child(loading_row("doc-view-docs-loading", dark));
    }
    let Some(result) = vm.result.as_ref() else {
        return column.child(
            row()
                .key("doc-view-no-documents")
                .w_full().grow(1.0).min_h(0.0).center()
                .child(text(t("docView.noDocuments")).wrap()),
        );
    };
    let columns: Vec<Column> = result.columns.iter().map(|name| Column::new(name.clone())).collect();
    let rows: Vec<Row> = result
        .rows
        .iter()
        .map(|row| Row::from(row.iter().map(|cell| CellValue::new(cell.clone()).mono()).collect::<Vec<_>>()))
        .collect();
    let mut table = DataTable::new("doc-view-result", &columns).rows(rows);
    if let Some(selected) = vm.selected_row_idx {
        let _ = selected;
    }
    if let Some(on_action) = on_action.clone() {
        table = table.on_select(move |s: &mut S, index| on_action(s, DocAction::SelectRow(index)));
    }
    column = column.child(table.build(dark));
    column
}

fn build_detail_editor<S: 'static>(
    vm: &DocumentViewVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let title = if vm.insert_mode { t("docView.insert") } else { t("docView.documentDetail") };
    let has_id = vm.has_document_id();
    let mut header = row()
        .key("doc-view-detail-header")
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .center_y()
        .child(text(title).key("doc-view-detail-title").wrap());

    let insert = Button::new(t("docView.insert"))
        .icon(&ICON_PLUS)
        .variant(if vm.insert_mode { ButtonVariant::Contrast } else { ButtonVariant::Ghost })
        .control_size(ControlSize::Small)
        .disabled(vm.mutating)
        .build(k)
        .key("doc-view-detail-insert")
        .on_press(emit(on_action, DocAction::InsertNew));
    header = header.child(insert);
    if !vm.insert_mode {
        header = header.child(
            Button::new(t("common.save"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Small)
                .disabled(vm.mutating || !has_id)
                .build(k)
                .key("doc-view-detail-save")
                .on_press(emit(on_action, DocAction::SaveDocument)),
        );
        header = header.child(
            Button::new(t("common.delete"))
                .icon(&ICON_TRASH)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .disabled(vm.mutating || !has_id)
                .build(k)
                .key("doc-view-detail-delete")
                .on_press(emit(on_action, DocAction::DeleteDocument)),
        );
    }
    header = header.child(
        row()
            .key("doc-view-detail-close")
            .role("button")
            .focusable(true)
            .w(20.0).h(20.0).center()
            .child(text("×").wrap())
            .on_press(emit(on_action, DocAction::CloseEditor)),
    );

    let mut detail = col()
        .key("doc-view-detail")
        .w(420.0)
        .shrink()
        .h_full()
        .min_h(0.0)
        .gap(8.0)
        .child(header);
    if !has_id && !vm.insert_mode {
        detail = detail.child(text(t("docView.noIdHint")).key("doc-view-detail-no-id").wrap());
    }
    if !vm.edit_error.is_empty() {
        detail = detail.child(CopyableError::new("doc-view-detail-error", vm.edit_error.clone()).build::<S>(k));
    }
    detail.child(
        TextField::new("doc-view-detail-edit", &TextEdit::new(vm.edit_text.clone()))
            .height(200.0)
            .disabled(true)
            .build(dark),
    )
}

fn build_queries<S: 'static>(
    vm: &DocumentViewVm,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let query = &vm.query;
    let execute = Button::new(t("query.execute"))
        .variant(ButtonVariant::Contrast)
        .control_size(ControlSize::Small)
        .disabled(query.running)
        .build(k)
        .key("doc-view-query-execute")
        .on_press(emit(on_action, DocAction::QueryExecute));
    let mut root = col().key("doc-view-queries").w_full().grow(1.0).min_h(0.0).min_w(0.0);
    root = root.child(
        row()
            .key("doc-view-query-header")
            .w_full().min_w(0.0).h(36.0).gap(8.0).center_y()
            .child(execute)
            .child(row().grow(1.0).min_w(0.0))
            .child(
                query.elapsed_ms.map(|ms| text(format!("{ms} ms")).wrap())
                    .unwrap_or_else(|| text(String::new())),
            ),
    );
    root = root.child(
        TextField::new("doc-view-query-editor", &TextEdit::new(query.sql.clone()))
            .height(120.0)
            .disabled(true)
            .build(dark),
    );
    if query.running && query.results.is_empty() {
        root = root.child(loading_row("doc-view-query-running", dark));
    }
    if !query.error.is_empty() && !query.running {
        root = root.child(CopyableError::new("doc-view-query-error", query.error.clone()).build::<S>(k));
    }
    if !query.results.is_empty() {
        let result = &query.results[query.active_result_idx.min(query.results.len() - 1)];
        let columns: Vec<Column> = result.columns.iter().map(|name| Column::new(name.clone())).collect();
        let rows: Vec<Row> = result.rows.iter().map(|row| Row::from(row.iter().map(|cell| CellValue::new(cell.clone()).mono()).collect::<Vec<_>>())).collect();
        root = root.child(DataTable::new("doc-view-query-result", &columns).rows(rows).build(dark));
    }
    if query.results.is_empty() && !query.running && query.error.is_empty() {
        root = root.child(
            row().key("doc-view-query-hint").w_full().grow(1.0).min_h(0.0).center()
                .child(text(t("docView.queryHint")).wrap()),
        );
    }
    root
}

fn loading_row(key: &str, dark: bool) -> El {
    row()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(8.0)
        .child(Spinner::new(format!("{key}-spinner")).size(SpinnerSize::Lg).build(dark))
        .child(text(t("common.loading")).wrap())
}
