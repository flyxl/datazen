//! `StructureColumnTable`, ported 1:1 from
//! `packages/frontend/src/windows/connection/structure/StructureColumnTable.tsx`.
//!
//! A form table over column drafts: name / type / nullable / primary key /
//! unique (create mode) / default / comment (when configured) plus a grip and
//! a remove button. Driver capability hints decide which controls are enabled;
//! every edit is emitted as a host action (no fake IPC). Rune has no native
//! drag payload, so drag intents carry the row index only.
//!
//! ## Removal stage
//!
//! No production host mounts this table yet; the structure editor will own the
//! drafts and apply the patches. Per-item allow annotations carry that removal
//! stage until then.

use std::collections::BTreeSet;
use std::rc::Rc;
use rune_kit::SortDir;

use rune::{col, icon, row, text, El, TextEdit};

use crate::i18n::t;
use crate::icons::{ICON_LAYOUT, ICON_TRASH};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

const ROOT: &str = "structure-column-table";
const DISABLED_KEY: &str = "structEditor.capDisabled";

/// React `mode: 'create' | 'alter'`.
#[allow(dead_code)] // VM mirror; consumed by the capability gates and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EditorMode {
    #[default]
    Create,
    Alter,
}

/// One `StructureCapabilityFlag` the table gates on.
#[allow(dead_code)] // VM mirror; consumed by cap_enabled and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    CreateTable,
    AddColumn,
    RenameColumn,
    AlterType,
    AlterNullability,
    AlterDefault,
    AlterPrimaryKey,
    CreateIndex,
    Comment,
    DropColumn,
    ReorderColumn,
}

/// React `StructureCapabilities` reduced to the flags this table reads.
#[allow(dead_code)] // VM surface; supplied by the driver capability feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StructureCapabilities {
    pub create_table: bool,
    pub add_column: bool,
    pub rename_column: bool,
    pub alter_type: bool,
    pub alter_nullability: bool,
    pub alter_default: bool,
    pub alter_primary_key: bool,
    pub create_index: bool,
    pub comment: bool,
    pub drop_column: bool,
    pub reorder_column: bool,
}

/// React `capEnabled(caps, control)`.
#[allow(dead_code)] // pure helper; consumed by the gates and tests.
pub fn cap_enabled(caps: Option<&StructureCapabilities>, control: Capability) -> bool {
    let Some(caps) = caps else { return false };
    match control {
        Capability::CreateTable => caps.create_table,
        Capability::AddColumn => caps.add_column,
        Capability::RenameColumn => caps.rename_column,
        Capability::AlterType => caps.alter_type,
        Capability::AlterNullability => caps.alter_nullability,
        Capability::AlterDefault => caps.alter_default,
        Capability::AlterPrimaryKey => caps.alter_primary_key,
        Capability::CreateIndex => caps.create_index,
        Capability::Comment => caps.comment,
        Capability::DropColumn => caps.drop_column,
        Capability::ReorderColumn => caps.reorder_column,
    }
}

/// React `controlDisabledKey` (always `structEditor.capDisabled`).
#[allow(dead_code)] // pure helper; consumed by disabled tooltips and tests.
pub fn control_disabled_key() -> &'static str {
    DISABLED_KEY
}

/// One editable cell's field, for the host's `TextEdit` accessor.
#[allow(dead_code)] // host seam; removed with the structure host wiring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnField {
    Name,
    DataType,
    DefaultValue,
    Comment,
}

/// One staged patch, mirroring React's `Partial<StructureColumnDraft>` writes.
#[allow(dead_code)] // host port; removed with the structure host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnPatch {
    Name(String),
    DataType(String),
    Nullable(bool),
    PrimaryKey(bool),
    Unique(bool),
    DefaultValue(Option<String>),
    Comment(Option<String>),
}

/// One `StructureColumnDraft` row.
#[allow(dead_code)] // VM mirror; supplied by the structure editor host.
#[derive(Clone, Debug, Default)]
pub struct ColumnDraft {
    pub id: String,
    pub name: TextEdit,
    pub data_type: TextEdit,
    pub nullable: bool,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub default_value: TextEdit,
    pub comment: TextEdit,
}

/// React `StructureEditorUiConfig` subset this table reads.
#[allow(dead_code)] // VM surface; supplied by the driver metadata.
#[derive(Clone, Debug, Default)]
pub struct UiConfig {
    pub show_comment: bool,
    pub column_types: Vec<String>,
    pub default_column_type: String,
}

/// Full view snapshot for one table.
#[allow(dead_code)] // VM surface; owned by the structure editor host.
#[derive(Clone, Debug, Default)]
pub struct StructureColumnTableVm {
    pub mode: EditorMode,
    pub caps: Option<StructureCapabilities>,
    pub ui_config: UiConfig,
    pub columns: Vec<ColumnDraft>,
    pub original_ids: BTreeSet<String>,
    pub reorder_enabled: bool,
    pub drag_idx: Option<usize>,
    pub sort: Option<(usize, SortDir)>,
}

impl StructureColumnTableVm {
    fn is_new(&self, column: &ColumnDraft) -> bool {
        !self.original_ids.contains(&column.id)
    }

    fn can(&self, column: &ColumnDraft, control: Capability) -> bool {
        let caps = self.caps.as_ref();
        if self.is_new(column) {
            if control == Capability::RenameColumn {
                return true;
            }
            if matches!(
                control,
                Capability::AlterType | Capability::AlterNullability | Capability::AlterDefault
            ) {
                return if self.mode == EditorMode::Create {
                    cap_enabled(caps, Capability::CreateTable)
                } else {
                    cap_enabled(caps, Capability::AddColumn)
                };
            }
        }
        cap_enabled(caps, control)
    }

    /// React's rendered row order after the host-owned sort.
    #[allow(dead_code)] // pure helper; consumed by the table and tests.
    pub fn row_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.columns.len()).collect();
        if let Some((column, dir)) = self.sort {
            order.sort_by(|&a, &b| {
                let left = self.columns.get(a).map_or("", |item| match column {
                    0 => item.name.text.as_str(),
                    1 => item.data_type.text.as_str(),
                    2 => item.default_value.text.as_str(),
                    _ => item.comment.text.as_str(),
                });
                let right = self.columns.get(b).map_or("", |item| match column {
                    0 => item.name.text.as_str(),
                    1 => item.data_type.text.as_str(),
                    2 => item.default_value.text.as_str(),
                    _ => item.comment.text.as_str(),
                });
                dir.apply(left.to_lowercase().cmp(&right.to_lowercase()))
            });
        }
        order
    }
}

/// Every intent the table emits.
#[allow(dead_code)] // host port; removed with the structure host wiring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureColumnAction {
    Update { id: String, patch: ColumnPatch },
    Remove(String),
    Select(usize),
    DragStart(usize),
    DragOver { target: usize },
    DragEnd,
    Sort(usize),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, StructureColumnAction) + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S, &str, ColumnField) -> &'b mut TextEdit>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: StructureColumnAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the table into one column.
#[allow(dead_code)] // consumer: the structure editor host (not landed).
pub fn build<S: 'static>(
    vm: &StructureColumnTableVm,
    edits: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let show_unique = vm.mode == EditorMode::Create;
    let show_comment = vm.ui_config.show_comment;
    let mut headers = vec![
        String::new(),
        t("structView.fieldName").to_owned(),
        t("structView.type").to_owned(),
        t("structView.nullable").to_owned(),
        t("structView.primaryKey").to_owned(),
    ];
    if show_unique {
        headers.push(t("structView.unique").to_owned());
    }
    headers.push(t("structView.defaultValue").to_owned());
    if show_comment {
        headers.push(t("structView.comment").to_owned());
    }
    headers.push(String::new());

    let mut head = row().key("structure-column-head").w_full().gap(8.0).center_y();
    for (index, label) in headers.iter().enumerate() {
        head = head.child(
            text(label.clone())
                .key(format!("structure-column-head-{index}"))
                .grow(1.0)
                .min_w(0.0)
                .color(k.c.text_muted),
        );
    }

    // A keyed sort strip stands in for React's native header menu; the table
    // itself stays a plain form grid, so sorting is host-owned state.
    let mut sort_strip = row().key("structure-sort-strip").w_full().gap(8.0).flex_wrap();
    for (index, label) in headers.iter().enumerate().skip(1) {
        sort_strip = sort_strip.child(
            text(label.clone())
                .key(format!("structure-sort-{index}"))
                .role("button")
                .focusable(true)
                .color(k.c.text_muted)
                .on_press(emit(&on_action, StructureColumnAction::Sort(index))),
        );
    }

    let mut body = col().key("structure-column-body").w_full().min_w(0.0);
    for original in vm.row_order() {
        let Some(column) = vm.columns.get(original) else { continue };
        body = body.child(column_row::<S>(
            vm,
            column,
            original,
            show_unique,
            show_comment,
            &edits,
            &on_action,
            dark,
        ));
    }

    col()
        .key(ROOT)
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .child(sort_strip)
        .child(head)
        .child(body)
}

#[allow(clippy::too_many_arguments)]
fn column_row<S: 'static>(
    vm: &StructureColumnTableVm,
    column: &ColumnDraft,
    index: usize,
    show_unique: bool,
    show_comment: bool,
    edits: &Option<EditFn<S>>,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let id = column.id.clone();
    let mut row_el = row()
        .key(format!("structure-column-row-{}", column.id))
        .w_full()
        .gap(8.0)
        .center_y()
        .on_press(emit(on_action, StructureColumnAction::Select(index)));

    let grip = icon(&ICON_LAYOUT, 14.0)
        .key(format!("structure-column-grip-{}", column.id))
        .color(k.c.text_muted)
        .help(if vm.reorder_enabled { String::new() } else { t(control_disabled_key()).to_owned() });
    row_el = row_el.child(grip);

    row_el = row_el.child(text_field::<S>(
        ColumnField::Name,
        &column.name,
        &column.id,
        vm.can(column, Capability::RenameColumn),
        edits,
        on_action,
        dark,
    ));
    row_el = row_el.child(text_field::<S>(
        ColumnField::DataType,
        &column.data_type,
        &column.id,
        vm.can(column, Capability::AlterType),
        edits,
        on_action,
        dark,
    ));

    let nullable = Checkbox::new(format!("structure-column-nullable-{}", column.id), column.nullable)
        .disabled(!vm.can(column, Capability::AlterNullability));
    let nullable = match on_action.clone() {
        Some(callback) if vm.can(column, Capability::AlterNullability) => {
            let id = id.clone();
            let checked = !column.nullable;
            nullable.on_toggle(move |s: &mut S| {
                callback(s, StructureColumnAction::Update {
                    id: id.clone(),
                    patch: ColumnPatch::Nullable(checked),
                })
            })
        }
        _ => nullable,
    };
    row_el = row_el.child(nullable.build(k));

    let pk = Checkbox::new(format!("structure-column-pk-{}", column.id), column.is_primary_key)
        .disabled(!vm.can(column, Capability::AlterPrimaryKey));
    let pk = match on_action.clone() {
        Some(callback) if vm.can(column, Capability::AlterPrimaryKey) => {
            let id = id.clone();
            let was_nullable = column.nullable;
            let checked = !column.is_primary_key;
            pk.on_toggle(move |s: &mut S| {
                callback(s, StructureColumnAction::Update {
                    id: id.clone(),
                    patch: ColumnPatch::PrimaryKey(checked),
                });
                if checked && was_nullable {
                    callback(s, StructureColumnAction::Update {
                        id: id.clone(),
                        patch: ColumnPatch::Nullable(false),
                    });
                }
            })
        }
        _ => pk,
    };
    row_el = row_el.child(pk.build(k));

    if show_unique {
        let unique = Checkbox::new(format!("structure-column-unique-{}", column.id), column.is_unique)
            .disabled(!vm.can(column, Capability::CreateIndex));
        let unique = match on_action.clone() {
            Some(callback) if vm.can(column, Capability::CreateIndex) => {
                let id = id.clone();
                let checked = !column.is_unique;
                unique.on_toggle(move |s: &mut S| {
                    callback(s, StructureColumnAction::Update {
                        id: id.clone(),
                        patch: ColumnPatch::Unique(checked),
                    })
                })
            }
            _ => unique,
        };
        row_el = row_el.child(unique.build(k));
    }

    row_el = row_el.child(text_field::<S>(
        ColumnField::DefaultValue,
        &column.default_value,
        &column.id,
        vm.can(column, Capability::AlterDefault),
        edits,
        on_action,
        dark,
    ));
    if show_comment {
        row_el = row_el.child(text_field::<S>(
            ColumnField::Comment,
            &column.comment,
            &column.id,
            vm.can(column, Capability::Comment),
            edits,
            on_action,
            dark,
        ));
    }

    let can_drop = vm.can(column, Capability::DropColumn) || vm.is_new(column);
    let mut remove = rune_kit::IconButton::new(&ICON_TRASH)
        .variant(rune_kit::IconButtonVariant::Ghost)
        .control_size(rune_kit::ControlSize::Mini)
        .label(t("structEditor.deleteColumn"))
        .disabled(!can_drop);
    if let Some(callback) = on_action.clone() {
        if can_drop {
            let id = id.clone();
            remove = remove.on_press(move |s: &mut S| {
                callback(s, StructureColumnAction::Remove(id.clone()))
            });
        }
    }
    row_el.child(
        remove
            .build(k)
            .key(format!("structure-column-remove-{}", column.id)),
    )
}

#[allow(clippy::too_many_arguments)]
fn text_field<S: 'static>(
    field: ColumnField,
    edit: &TextEdit,
    id: &str,
    enabled: bool,
    edits: &Option<EditFn<S>>,
    _on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let field_name = match field {
        ColumnField::Name => "structure-column-name-",
        ColumnField::DataType => "structure-column-type-",
        ColumnField::DefaultValue => "structure-column-default-",
        ColumnField::Comment => "structure-column-comment-",
    };
    let key = format!("{field_name}{id}");
    let id = id.to_owned();
    let mut input = TextField::new(&key, edit).grow().height(28.0);
    if enabled {
        if let Some(accessor) = edits.clone() {
            let accessor = accessor.clone();
            input = input.on_edit(move |s: &mut S| accessor(s, &id, field));
        } else {
            input = input.disabled(true);
        }
    } else {
        // React sets disabled={!canRename} etc.; a capability-denied input
        // must not take a caret or mutate the draft.
        input = input.disabled(true);
    }
    input.build(dark)
}
