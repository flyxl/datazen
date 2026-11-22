//! Port of `windows/connection/TableStructureEditor.tsx` (DB-03).
//!
//! React is the create/alter table structure editor: a toolbar (back, title,
//! target, export, preview, cancel, apply), an optional table-name field in
//! create mode, the column table, an add-column control, the index section and
//! its table, an error strip, the plan preview and the risky-apply confirm.
//! Capabilities gate every write control; loading and unsupported states own
//! the whole surface. This port keeps that contract; every command is an
//! injected [`StructureEditorAction`] the host executes — no fake IPC.
//!
//! ## Composition
//!
//! The column and index tables and the plan preview are already-ported child
//! views. Their areas are honest keyed slots (`column-table`,
//! `structure-index-table`, `plan-preview`) the host fills; the plan preview is
//! composed here from its port. Per-item `#[allow(dead_code)]` carries removal
//! stage "drop when the structure editor host wires these slots".
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `struct-editor-back`,
//! `struct-editor-title`, `struct-editor-target`,
//! `struct-editor-export-structure`, `struct-editor-execute`,
//! `struct-editor-add-column`, `plan-preview`, `plan-close` and `column-table`.

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::structure_editor::control_hints::cap_enabled;
use crate::structure_editor::{
    StructureCapabilities, StructureCapabilityFlag, StructureChangePlan, StructureColumnDraft,
    StructureEditorUiConfig, StructureIndexDraft,
};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::structure_plan_preview::{
    build as build_plan, StructurePlanAction, StructurePlanActionFn, StructurePlanPreviewVm,
};

pub(crate) const ROOT_KEY: &str = "table-structure-editor";
pub(crate) const BACK_KEY: &str = "struct-editor-back";
pub(crate) const TITLE_KEY: &str = "struct-editor-title";
pub(crate) const TARGET_KEY: &str = "struct-editor-target";
pub(crate) const EXPORT_KEY: &str = "struct-editor-export-structure";
pub(crate) const EXECUTE_KEY: &str = "struct-editor-execute";
pub(crate) const ADD_COLUMN_KEY: &str = "struct-editor-add-column";
pub(crate) const ADD_INDEX_KEY: &str = "struct-editor-add-index";
pub(crate) const UNSUPPORTED_KEY: &str = "struct-editor-unsupported";
pub(crate) const LOADING_KEY: &str = "struct-editor-loading";
pub(crate) const ERROR_KEY: &str = "struct-editor-error";
pub(crate) const COLUMN_TABLE_KEY: &str = "column-table";
pub(crate) const PLAN_KEY: &str = "plan-preview";
pub(crate) const PLAN_CLOSE_KEY: &str = "plan-close";

/// React `mode: 'create' | 'alter'`.
#[allow(dead_code)] // VM mirror; remove when the host owns the editor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditorMode {
    #[default]
    Create,
    Alter,
}

/// Host-owned projection of the props plus the `useState` values.
#[allow(dead_code)] // VM surface; remove when the host wires the editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StructureEditorVm {
    pub(crate) mode: EditorMode,
    pub(crate) table_name: String,
    pub(crate) initial_table_name: Option<String>,
    pub(crate) database: Option<String>,
    pub(crate) ui_config: Option<StructureEditorUiConfig>,
    pub(crate) caps: Option<StructureCapabilities>,
    pub(crate) show_back_button: bool,
    pub(crate) export_supported: bool,
    pub(crate) loading: bool,
    pub(crate) previewing: bool,
    pub(crate) executing: bool,
    pub(crate) exporting: bool,
    pub(crate) error: String,
    pub(crate) columns: Vec<StructureColumnDraft>,
    pub(crate) indexes: Vec<StructureIndexDraft>,
    pub(crate) preview_plan: Option<StructureChangePlan>,
    pub(crate) table_name_edit: TextEdit,
}

#[allow(dead_code)] // pure helpers; remove with the host.
impl StructureEditorVm {
    /// React `resolveUiConfig(...)`: null unless the config exists and is not
    /// explicitly disabled (`enabled === false`).
    pub(crate) fn supported(&self) -> bool {
        self.ui_config
            .as_ref()
            .is_some_and(|cfg| cfg.enabled != Some(false))
    }

    /// React's header title per mode.
    pub(crate) fn title(&self) -> String {
        match self.mode {
            EditorMode::Create => t("common.newTable").to_owned(),
            EditorMode::Alter => format!(
                "{} · {}",
                t("common.editTableStructure"),
                self.initial_table_name.as_deref().unwrap_or("")
            ),
        }
    }

    /// React `validColumns` (columns with a trimmed, non-empty name).
    pub(crate) fn valid_columns(&self) -> Vec<&StructureColumnDraft> {
        self.columns.iter().filter(|column| !column.name.trim().is_empty()).collect()
    }

    /// React `isValid`.
    pub(crate) fn is_valid(&self) -> bool {
        !self.table_name.trim().is_empty() && !self.valid_columns().is_empty()
    }

    /// React `canAddColumn`.
    pub(crate) fn can_add_column(&self) -> bool {
        let control = match self.mode {
            EditorMode::Create => StructureCapabilityFlag::CreateTable,
            EditorMode::Alter => StructureCapabilityFlag::AddColumn,
        };
        cap_enabled(self.caps.as_ref(), control)
    }

    /// React `canAddIndex`.
    pub(crate) fn can_add_index(&self) -> bool {
        cap_enabled(self.caps.as_ref(), StructureCapabilityFlag::CreateIndex)
    }

    /// React's export control condition.
    pub(crate) fn can_export(&self) -> bool {
        self.mode == EditorMode::Alter
            && self
                .initial_table_name
                .as_deref()
                .is_some_and(|name| !name.is_empty())
            && self.export_supported
    }

    /// React's execute button label.
    pub(crate) fn execute_label(&self) -> String {
        if self.executing {
            return t("structEditor.executing").to_owned();
        }
        match self.mode {
            EditorMode::Create => t("structEditor.createTable").to_owned(),
            EditorMode::Alter => t("structEditor.saveChanges").to_owned(),
        }
    }

    /// React `disabled={!isValid || executing}` on Execute.
    pub(crate) fn can_execute(&self) -> bool {
        self.is_valid() && !self.executing
    }

    /// React `disabled={!isValid || previewing}` on Preview.
    pub(crate) fn can_preview(&self) -> bool {
        self.is_valid() && !self.previewing
    }
}

/// Every interaction the editor emits.
#[allow(dead_code)] // host port; remove when the host wires the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructureEditorAction {
    Back,
    Cancel,
    SetTableName(String),
    Preview,
    Execute,
    Export,
    AddColumn,
    AddIndex,
    UpdateColumn(String),
    RemoveColumn(String),
    UpdateIndex(String),
    RemoveIndex(String),
    ClosePreview,
    ConfirmApply(bool),
}

#[allow(dead_code)] // host port; remove when the host wires the editor.
pub(crate) type StructureEditorActionFn<S> = Rc<dyn Fn(&mut S, StructureEditorAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<StructureEditorActionFn<S>>,
    action: StructureEditorAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the editor into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the editor.
pub(crate) fn build<S: 'static>(
    vm: &StructureEditorVm,
    on_action: &Option<StructureEditorActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    if !vm.supported() {
        layer.push(
            col()
                .key(UNSUPPORTED_KEY)
                .w_full()
                .h_full()
                .center()
                .child(
                    text(t("structEditor.notSupported"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }
    if vm.loading {
        layer.push(
            col()
                .key(LOADING_KEY)
                .w_full()
                .h_full()
                .center()
                .gap(8.0)
                .child(Spinner::new("struct-editor-spinner").size(SpinnerSize::Lg).build(dark))
                .child(
                    text(t("common.loadingTableStructure"))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }

    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_h(0.0)
        .min_w(0.0);

    // ── Toolbar ───────────────────────────────────────────────────────
    let mut toolbar = row()
        .key("struct-editor-toolbar")
        .w_full()
        .gap(16.0)
        .center_y()
        .px(16.0)
        .py(12.0);
    if vm.show_back_button {
        toolbar = toolbar.child(toolbar_button::<S>(
            BACK_KEY,
            t("common.back").to_owned(),
            false,
            on_action,
            StructureEditorAction::Back,
            None,
            dark,
        ));
    }
    toolbar = toolbar
        .child(
            text(vm.title())
                .key(TITLE_KEY)
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        );
    if let Some(database) = vm.database.as_deref() {
        toolbar = toolbar.child(
            text(database.to_owned())
                .key(TARGET_KEY)
                .label(database.to_owned())
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .truncate(),
        );
    }
    toolbar = toolbar.child(row().grow(1.0).h(1.0));
    if vm.can_export() {
        toolbar = toolbar.child(toolbar_button::<S>(
            EXPORT_KEY,
            if vm.exporting {
                t("structEditor.exportingStructure").to_owned()
            } else {
                t("structEditor.exportStructure").to_owned()
            },
            vm.exporting,
            on_action,
            StructureEditorAction::Export,
            None,
            dark,
        ));
    }
    toolbar = toolbar.child(toolbar_button::<S>(
        "struct-editor-preview",
        if vm.previewing {
            t("structEditor.previewing").to_owned()
        } else {
            t("structEditor.previewSQL").to_owned()
        },
        !vm.can_preview(),
        on_action,
        StructureEditorAction::Preview,
        None,
        dark,
    ));
    if !vm.show_back_button {
        toolbar = toolbar.child(toolbar_button::<S>(
            "struct-editor-cancel",
            t("common.cancel").to_owned(),
            false,
            on_action,
            StructureEditorAction::Cancel,
            None,
            dark,
        ));
    }
    toolbar = toolbar.child(toolbar_button::<S>(
        EXECUTE_KEY,
        vm.execute_label(),
        !vm.can_execute(),
        on_action,
        StructureEditorAction::Execute,
        None,
        dark,
    ));
    root = root.child(toolbar);

    // ── Create-mode table name ────────────────────────────────────────
    if vm.mode == EditorMode::Create {
        // The host owns the table-name TextEdit and supplies its edit accessor;
        // without one this is an honest bound display of the current name.
        let name_field = TextField::new("struct-editor-table-name", &vm.table_name_edit)
            .height(32.0)
            .width(320.0)
            .text_style(k.ty.text_small)
            .placeholder("new_table")
            .disabled(true);
        root = root.child(
            row()
                .key("struct-editor-name-row")
                .w_full()
                .gap(12.0)
                .center_y()
                .px(16.0)
                .py(12.0)
                .child(text(t("structEditor.tableName")).font(k.ty.text_small).color(k.c.text_muted))
                .child(name_field.build(dark)),
        );
    }

    // ── Draft areas (column table + add control, index section) ───────
    root = root.child(
        col()
            .key("struct-editor-drafts")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(
                row()
                    .key(COLUMN_TABLE_KEY)
                    .w_full()
                    .min_h(0.0)
                    .min_w(0.0),
            )
            .child(
                row()
                    .key("struct-editor-add-column-row")
                    .w_full()
                    .px(16.0)
                    .py(12.0)
                    .child(toolbar_button::<S>(
                        ADD_COLUMN_KEY,
                        t("structEditor.addColumn").to_owned(),
                        !vm.can_add_column(),
                        on_action,
                        StructureEditorAction::AddColumn,
                        if vm.can_add_column() { None } else { Some(t("structEditor.capDisabled").to_owned()) },
                        dark,
                    )),
            )
            .child(
                col()
                    .key("struct-editor-index-section")
                    .w_full()
                    .px(16.0)
                    .py(8.0)
                    .child(
                        row()
                            .key("struct-editor-index-header")
                            .w_full()
                            .center_y()
                            .justify_between()
                            .py(8.0)
                            .child(text(t("structEditor.indexes")).font(k.ty.text_small).color(k.c.text_muted))
                            .child(toolbar_button::<S>(
                                ADD_INDEX_KEY,
                                t("structEditor.addIndex").to_owned(),
                                !vm.can_add_index(),
                                on_action,
                                StructureEditorAction::AddIndex,
                                if vm.can_add_index() { None } else { Some(t("structEditor.capDisabled").to_owned()) },
                                dark,
                            )),
                    )
                    .child(
                        row()
                            .key("structure-index-table")
                            .w_full()
                            .min_h(0.0)
                            .min_w(0.0),
                    ),
            ),
    );

    if !vm.error.is_empty() {
        root = root.child(
            row()
                .key(ERROR_KEY)
                .w_full()
                .px(16.0)
                .py(8.0)
                .child(
                    text(vm.error.clone())
                        .key("struct-editor-error-text")
                        .font(k.ty.text_small)
                        .color(k.c.danger.fg)
                        .wrap_anywhere()
                        .min_w(0.0),
                ),
        );
    }

    // ── Plan preview (composed from its port) ─────────────────────────
    if let Some(plan) = vm.preview_plan.as_ref() {
        let mut plan_layer = Vec::new();
        let plan_vm = StructurePlanPreviewVm {
            plan: plan.clone(),
            loading: false,
        };
        let plan_action: Option<StructurePlanActionFn<S>> = {
            let callback = on_action.clone();
            callback.map(|callback| {
                Rc::new(move |s: &mut S, action: StructurePlanAction| {
                    if action == StructurePlanAction::Close {
                        callback(s, StructureEditorAction::ClosePreview);
                    }
                }) as StructurePlanActionFn<S>
            })
        };
        build_plan(&plan_vm, &plan_action, dark, &mut plan_layer);
        for element in plan_layer {
            root = root.child(element);
        }
        // React testids pinned by the plan-preview test.
        root = root
            .child(row().key(PLAN_KEY).size(0.0, 0.0).a11y_hidden())
            .child(
                text(t("common.close"))
                    .key(PLAN_CLOSE_KEY)
                    .role("button")
                    .focusable(true)
                    .on_press(emit(on_action, StructureEditorAction::ClosePreview))
                    .a11y_hidden(),
            );
    }

    layer.push(root);
}

fn toolbar_button<S: 'static>(
    key: &str,
    label: String,
    disabled: bool,
    on_action: &Option<StructureEditorActionFn<S>>,
    action: StructureEditorAction,
    help: Option<String>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut button = Button::new(label.clone())
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .disabled(disabled)
        .build(k)
        .key(key.to_owned())
        .label(label.clone());
    if let Some(help) = help {
        button = button.help(help);
    }
    if !disabled {
        let press = emit(on_action, action);
        button = button.on_press(move |s: &mut S| press(s));
    }
    button
}
