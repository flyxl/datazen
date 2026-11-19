//! Port of `packages/frontend/src/windows/connection/ContentViewDialogs.tsx`.
//!
//! React's component is a pure host: seven child dialogs (export, batch export,
//! import, execute SQL file, create database / schema / user) whose open flags
//! stay owned by `ContentView`. This module keeps that seam with [`DialogsVm`]
//! and an injected [`DialogAction`] callback; no IPC or store is faked.
//!
//! The seven child dialogs are not ported in this lane. Each one is represented
//! by a real [`ui::dialog::Dialog`] shell with its React title, open gate and
//! callback wiring, and an honest context row (the values the host supplied).
//! Removal stage for every item: when the corresponding child dialog port
//! lands, replace that shell with the real component; the keys stay the same.
//!
//! Keys: `content-view-export-dialog`, `content-view-batch-export-dialog`,
//! `content-view-import-dialog`, `content-view-sql-file-dialog`,
//! `content-view-create-database-dialog`, `content-view-create-schema-dialog`,
//! `content-view-create-user-dialog`; each with `-body`, `-context`, `-cancel`,
//! `-confirm`, plus React's `sql-file-execute` / `sql-file-status`.

use std::rc::Rc;

use rune::tok::{self, type_};
use rune::{row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::views::create_database_dialog::{
    build_snapshot as build_db_snapshot, AdminCreateAction, AdminCreateState, DB_SPEC,
};
use crate::views::create_schema_dialog::build_schema_snapshot;
use crate::views::create_user_dialog::{
    build_snapshot as build_user_snapshot, CreateUserAction, CreateUserState,
};

/// React's export capability union (mirrors `lib/exportCapability.ts`).
#[allow(dead_code)] // VM surface; removed when the child dialog port lands
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportCapability {
    None,
    LoadedOnly,
    #[default]
    FullTable,
}

/// React `ConnectionContext` fields the dialogs actually read.
#[allow(dead_code)] // VM surface; removed when the child dialog port lands
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DialogConnectionCtx {
    pub(crate) db_session_id: String,
    pub(crate) connection_id: String,
    pub(crate) connection_name: String,
    pub(crate) database_type: String,
}

/// React `ContentViewDialogsProps` as a cloneable value.
#[allow(dead_code)] // VM surface; removed when the child dialog port lands
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DialogsVm {
    pub(crate) conn_ctx: Option<DialogConnectionCtx>,
    pub(crate) current_database: Option<String>,
    pub(crate) initial_database: Option<String>,
    pub(crate) export_capability: ExportCapability,
    pub(crate) export_open: bool,
    pub(crate) export_table_name: Option<String>,
    pub(crate) table_columns: Vec<String>,
    pub(crate) table_rows: usize,
    pub(crate) selected_rows: usize,
    pub(crate) total_rows: u64,
    pub(crate) batch_export_open: bool,
    pub(crate) batch_export_initial_selected: Vec<String>,
    pub(crate) exportable_table_names: Vec<String>,
    pub(crate) import_open: bool,
    pub(crate) import_table_name: Option<String>,
    pub(crate) import_schema: Option<String>,
    pub(crate) import_database: Option<String>,
    pub(crate) sql_file_open: bool,
    pub(crate) create_db_open: bool,
    pub(crate) create_schema_open: bool,
    pub(crate) create_user_open: bool,
}

/// The callbacks React hands to the child dialogs, flattened into one enum a
/// host can dispatch. The `onCreated` variants stand for the post-success side
/// effects (schema reload / privileges panel refresh).
#[allow(dead_code)] // VM surface; removed when the child dialog port lands
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogAction {
    CloseExport,
    CloseBatchExport,
    CloseImport {
        table: String,
        schema: Option<String>,
        database: Option<String>,
    },
    CloseSqlFile,
    Executed,
    CloseCreateDb,
    DbCreated,
    CloseCreateSchema,
    SchemaCreated,
    CloseCreateUser,
    UserCreated,
}

/// Compose every open dialog into `layer` (a closed React `Dialog` mounts
/// nothing, so the open gate is applied here too).
#[allow(dead_code)] // reusable component; mounted by views::content_view
pub(crate) fn build<S: 'static>(
    vm: &DialogsVm,
    dark: bool,
    layer: &mut Vec<El>,
    on_action: &Rc<dyn Fn(&mut S, DialogAction)>,
) {
    let Some(ctx) = vm.conn_ctx.as_ref() else {
        return;
    };
    let k: &'static Kit = kit(dark);

    // Export: React gates on all three of open / tableName / connCtx.
    if vm.export_open {
        if let Some(table) = vm.export_table_name.as_deref() {
            let body = format!(
                "{} · {} · {} · {} / {}",
                table,
                database_of(vm),
                vm.total_rows,
                vm.selected_rows,
                vm.table_columns.len()
            );
            shell(
                k,
                layer,
                "content-view-export-dialog",
                t("common.exportData"),
                on_action,
                &body,
                None,
                Button::new(t("export.export"))
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::ExtraLarge)
                    .on_press(close(on_action, DialogAction::CloseExport))
                    .build(k),
                dark,
            );
        }
    }

    if vm.batch_export_open {
        let body = format!(
            "{} · {} · {}",
            database_of(vm),
            vm.exportable_table_names.len(),
            vm.batch_export_initial_selected.len()
        );
        shell(
            k,
            layer,
            "content-view-batch-export-dialog",
            t("batchExport.title"),
            on_action,
            &body,
            None,
            Button::new(t("export.export"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(close(on_action, DialogAction::CloseBatchExport))
                .build(k),
            dark,
        );
    }

    if vm.import_open {
        let table = vm.import_table_name.clone().unwrap_or_default();
        let body = format!(
            "{} · {} · {}",
            table,
            vm.import_schema.as_deref().unwrap_or(""),
            vm.import_database.as_deref().unwrap_or("")
        );
        let import_action = DialogAction::CloseImport {
            table,
            schema: vm.import_schema.clone(),
            database: vm.import_database.clone(),
        };
        shell(
            k,
            layer,
            "content-view-import-dialog",
            t("common.importData"),
            on_action,
            &body,
            Some("import-summary"),
            Button::new(t("import.import"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::ExtraLarge)
                .on_press(close(on_action, import_action))
                .build(k),
            dark,
        );
    }

    if vm.sql_file_open {
        let body = format!(
            "{} · {} · {}",
            ctx.connection_name,
            database_of(vm),
            ctx.db_session_id
        );
        shell(
            k,
            layer,
            "content-view-sql-file-dialog",
            t("common.executeSqlFile"),
            on_action,
            &body,
            Some("sql-file-status"),
            row().child(
                Button::new(t("sqlFile.selectAndRun"))
                    .variant(ButtonVariant::Contrast)
                    .control_size(ControlSize::ExtraLarge)
                    .on_press(close(on_action, DialogAction::Executed))
                    .build(k)
                    .key("sql-file-execute"),
            ),
            dark,
        );
    }

    // T266: the real create dialogs replace the structural shells. The host
    // here owns only the open flags, so it supplies a snapshot form; the action
    // adapter maps the form's host calls onto T223's existing callback surface.
    if vm.create_db_open {
        let adapter: Rc<dyn Fn(&mut S, AdminCreateAction)> = {
            let on_action = on_action.clone();
            Rc::new(move |s: &mut S, action: AdminCreateAction| match action {
                AdminCreateAction::Close => on_action(s, DialogAction::CloseCreateDb),
                AdminCreateAction::Execute { .. } | AdminCreateAction::Created { .. } => {
                    on_action(s, DialogAction::DbCreated)
                }
            })
        };
        let form = AdminCreateState {
            name: TextEdit::new("new_database".to_owned()),
            ..Default::default()
        };
        let context = format!("{} · {}", ctx.connection_name, ctx.database_type);
        build_db_snapshot::<S>(&form, DB_SPEC, Some(&context), Some(adapter), dark, layer);
    }

    if vm.create_schema_open {
        let adapter: Rc<dyn Fn(&mut S, AdminCreateAction)> = {
            let on_action = on_action.clone();
            Rc::new(move |s: &mut S, action: AdminCreateAction| match action {
                AdminCreateAction::Close => on_action(s, DialogAction::CloseCreateSchema),
                AdminCreateAction::Execute { .. } | AdminCreateAction::Created { .. } => {
                    on_action(s, DialogAction::SchemaCreated)
                }
            })
        };
        let form = AdminCreateState {
            name: TextEdit::new("new_schema".to_owned()),
            ..Default::default()
        };
        build_schema_snapshot::<S>(&form, Some(database_of(vm)), Some(adapter), dark, layer);
    }

    // T286: the real create-user dialog replaces the structural shell. The
    // adapter maps the form's host calls onto T223's existing callback surface.
    if vm.create_user_open {
        let adapter: Rc<dyn Fn(&mut S, CreateUserAction)> = {
            let on_action = on_action.clone();
            Rc::new(move |s: &mut S, action: CreateUserAction| match action {
                CreateUserAction::Close => on_action(s, DialogAction::CloseCreateUser),
                CreateUserAction::Create { .. }
                | CreateUserAction::Grant { .. }
                | CreateUserAction::Created { .. }
                | CreateUserAction::Skip { .. } => on_action(s, DialogAction::UserCreated),
            })
        };
        let form = CreateUserState {
            username: TextEdit::new("new_user".to_owned()),
            ..Default::default()
        };
        let context = format!("{} · {}", ctx.connection_name, database_of(vm));
        build_user_snapshot::<S>(&form, Some(&context), Some(adapter), dark, layer);
    }
}

fn database_of(vm: &DialogsVm) -> &str {
    vm.current_database
        .as_deref()
        .or(vm.initial_database.as_deref())
        .unwrap_or("")
}

fn close<S: 'static>(
    on_action: &Rc<dyn Fn(&mut S, DialogAction)>,
    action: DialogAction,
) -> impl Fn(&mut S) + Clone {
    let on_action = on_action.clone();
    move |s: &mut S| on_action(s, action.clone())
}

fn shell<S: 'static>(
    k: &'static Kit,
    layer: &mut Vec<El>,
    key: &str,
    title: &str,
    on_action: &Rc<dyn Fn(&mut S, DialogAction)>,
    context: &str,
    context_key: Option<&str>,
    confirm: El,
    dark: bool,
) {
    let theme = tok::theme(dark);
    let text_key = context_key
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{key}-context"));
    let mut dialog = Dialog::new(key, title)
        .open(true)
        .fit()
        .width(480.0)
        .body_max_height(280.0)
        .child(row().key(format!("{key}-body")).w_full().min_w(0.0).child(
            text(context.to_owned())
                .key(text_key)
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .wrap()
                .min_w(0.0),
        ))
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::ExtraLarge)
                .on_press(close(on_action, close_action(key)))
                .build(k)
                .key(format!("{key}-cancel")),
        )
        .footer(confirm.key(format!("{key}-confirm")));
    dialog = dialog.on_close(close(on_action, close_action(key)));
    dialog.build(k, layer);
}

/// Map a dialog key to its close callback (Escape / header close).
fn close_action(key: &str) -> DialogAction {
    match key {
        "content-view-export-dialog" => DialogAction::CloseExport,
        "content-view-batch-export-dialog" => DialogAction::CloseBatchExport,
        "content-view-import-dialog" => DialogAction::CloseImport {
            table: String::new(),
            schema: None,
            database: None,
        },
        "content-view-sql-file-dialog" => DialogAction::CloseSqlFile,
        "content-view-create-database-dialog" => DialogAction::CloseCreateDb,
        "content-view-create-schema-dialog" => DialogAction::CloseCreateSchema,
        _ => DialogAction::CloseCreateUser,
    }
}
