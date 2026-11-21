//! Port of `packages/frontend/src/windows/connection/StructureView.tsx`.
//!
//! React is a read-only table-structure panel: a table-name header with an
//! optional Edit Structure button, a six-column column table (name / type /
//! nullable / default / key badges / comment) and a summary bar. It loads a
//! cached schema and highlights a `targetColumn` case-insensitively (with a
//! timed scroll+fade). The Rune port keeps every test id, i18n key, row and
//! state: the schema snapshot and highlight are host-fed VM fields, and the
//! edit intent is a [`StructureViewAction`] on an injected host port — no fake
//! IPC, timeout or promise lives here.
//!
//! ## Keys
//!
//! `struct-edit-structure` is React's `data-testid`, kept verbatim. Each row
//! carries the `struct-col-{name}` key (React's `data-struct-col="{name}"`); the
//! `struct-view-*` keys name the other Rune-side handles for tests.
//!
//! ## Documented deviations
//!
//! * `dataTypeTextClass` (per-SQL-type text colour) is reduced to the kit's
//!   mono text tokens; the type text is still shown verbatim.
//! * `lucide Pencil` is not in the registry; `ICON_LAYOUT` stands in for it.
//! * React's 150ms scroll / 1400ms fade are host-owned timers; the port renders
//!   the `highlighted_col` the host supplies.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this view yet; the connection-page host owns the
//! VM, loads the schema and maps the edit intent. Until then every reusable
//! item carries a per-item `#[allow(dead_code)]` with this stage.

use std::rc::Rc;

use rune::{col, icon, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_LAYOUT, ICON_LOADER2};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

const ROOT: &str = "structure-view";
pub(crate) const EDIT_KEY: &str = "struct-edit-structure";
pub(crate) const TABLE_KEY: &str = "structure-view-table";
pub(crate) const SUMMARY_KEY: &str = "structure-view-summary";
pub(crate) const LOADING_KEY: &str = "structure-view-loading";
pub(crate) const ERROR_KEY: &str = "structure-view-error";

/// React `TableSchema.column`, reduced to what the view paints.
#[allow(dead_code)] // VM mirror; supplied by the host schema feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructColumnVm {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_auto_increment: bool,
}

/// React `TableSchema.index`, reduced to the unique/primary/columns fields.
#[allow(dead_code)] // VM mirror; supplied by the host schema feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructIndexVm {
    pub is_unique: bool,
    pub is_primary: bool,
    pub columns: Vec<String>,
}

/// React `TableSchema`, reduced to what `StructureView` reads.
#[allow(dead_code)] // VM mirror; supplied by the host schema feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TableSchemaVm {
    pub table_name: String,
    pub columns: Vec<StructColumnVm>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<StructIndexVm>,
    pub foreign_keys: Vec<String>,
}

impl TableSchemaVm {
    /// React `uniqueCols`: columns of unique, non-primary indexes.
    #[allow(dead_code)] // pure rule; consumed by the badge column and tests.
    pub(crate) fn unique_columns(&self) -> Vec<String> {
        self.indexes
            .iter()
            .filter(|index| index.is_unique && !index.is_primary)
            .flat_map(|index| index.columns.iter().cloned())
            .collect()
    }

    /// React `indexes.filter(i => i.isUnique && !i.isPrimary).length`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn unique_index_count(&self) -> usize {
        self.indexes.iter().filter(|index| index.is_unique && !index.is_primary).count()
    }
}

/// Host-owned projection of the props + the loaded schema + highlight.
#[allow(dead_code)] // VM surface; owned by the connection-page host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StructureViewVm {
    pub db_session_id: String,
    pub table_name: String,
    pub database: String,
    pub target_column: Option<String>,
    /// Whether the host wired `onEditStructure`.
    pub can_edit: bool,
    pub loading: bool,
    pub error: String,
    pub schema: Option<TableSchemaVm>,
    /// The host-owned highlight (React's `highlightedCol` after the fade timer).
    pub highlighted_col: Option<String>,
}

impl StructureViewVm {
    /// React's target-column match: first column whose lowercased name equals
    /// the lowercased `targetColumn`.
    #[allow(dead_code)] // pure helper; consumed by the row highlight and tests.
    pub(crate) fn matched_column(&self) -> Option<&str> {
        let target = self.target_column.as_deref()?.to_lowercase();
        let schema = self.schema.as_ref()?;
        schema.columns.iter().find(|col| col.name.to_lowercase() == target).map(|col| col.name.as_str())
    }

    /// The effective highlight: the host-owned `highlighted_col` is the sole
    /// source. React clears it after the fade and never re-derives it from
    /// `targetColumn` (which only seeds it on initial load).
    #[allow(dead_code)] // pure helper; consumed by the row highlight and tests.
    pub(crate) fn effective_highlight(&self) -> Option<String> {
        self.highlighted_col.clone()
    }

    /// React `t('structView.fields', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn fields_text(&self) -> String {
        let count = self.schema.as_ref().map_or(0, |schema| schema.columns.len());
        t("structView.fields").replace("{count}", &count.to_string())
    }

    /// React `t('structView.primaryKeys', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn primary_keys_text(&self) -> String {
        let count = self.schema.as_ref().map_or(0, |schema| schema.primary_keys.len());
        t("structView.primaryKeys").replace("{count}", &count.to_string())
    }

    /// React `t('structView.uniqueIndexes', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn unique_indexes_text(&self) -> String {
        let count = self.schema.as_ref().map_or(0, TableSchemaVm::unique_index_count);
        t("structView.uniqueIndexes").replace("{count}", &count.to_string())
    }

    /// React `t('common.indexCount', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn index_count_text(&self) -> String {
        let count = self.schema.as_ref().map_or(0, |schema| schema.indexes.len());
        t("common.indexCount").replace("{count}", &count.to_string())
    }

    /// React `t('common.foreignKeyCount', { count })`.
    #[allow(dead_code)] // pure helper; consumed by the summary bar and tests.
    pub(crate) fn foreign_key_count_text(&self) -> String {
        let count = self.schema.as_ref().map_or(0, |schema| schema.foreign_keys.len());
        t("common.foreignKeyCount").replace("{count}", &count.to_string())
    }
}

/// The single host intent the view emits.
#[allow(dead_code)] // host port; remove when the connection host wires the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StructureViewAction {
    EditStructure { table_name: String },
}

type ActionFn<S> = Rc<dyn Fn(&mut S, StructureViewAction) + 'static>;

/// Build the view.
#[allow(dead_code)] // view entry; remove when the connection host wires the view.
pub(crate) fn build<S: 'static>(
    vm: &StructureViewVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    // React's guard order: loading → error → !schema (renders null).
    if vm.loading {
        return loading_body(dark);
    }
    if !vm.error.is_empty() {
        return error_body::<S>(&vm.error, dark);
    }
    let schema = match &vm.schema {
        Some(schema) => schema,
        None => return col().key("structure-view-none").w_full().h_full().a11y_hidden(),
    };

    let k = kit(dark);
    let highlight = vm.effective_highlight();
    let pk = &schema.primary_keys;
    let unique = schema.unique_columns();

    let mut root = col().key(ROOT).w_full().h_full().min_h(0.0).gap(0.0);

    // ── Header ──────────────────────────────────────────────────────────────
    let mut header = row()
        .key("structure-view-header")
        .w_full()
        .px(14.0)
        .py(12.0)
        .gap(8.0)
        .center_y()
        .child(
            text(schema.table_name.clone())
                .key("structure-view-table-name")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(row().grow(1.0));
    if vm.can_edit {
        let mut edit = Button::new(t("common.editStructure"))
            .variant(ButtonVariant::Outline)
            .control_size(ControlSize::Small)
            .icon(&ICON_LAYOUT);
        if let Some(on_action) = on_action.clone() {
            let table = vm.table_name.clone();
            edit = edit.on_press(move |s: &mut S| {
                on_action(s, StructureViewAction::EditStructure { table_name: table.clone() })
            });
        }
        header = header.child(edit.build(k).key(EDIT_KEY));
    }
    root = root.child(header);

    // ── Column table ────────────────────────────────────────────────────────
    let header_row = row()
        .key("structure-view-col-header")
        .w_full()
        .gap(0.0)
        .center_y()
        .fill(k.c.layer_02);
    let header_row = [t("structView.fieldName"), t("structView.type"), t("structView.nullable"),
                      t("structView.defaultValue"), t("structView.primaryKey"), t("structView.comment")]
        .into_iter()
        .enumerate()
        .fold(header_row, |acc, (index, label)| {
            acc.child(
                text(label)
                    .key(format!("structure-view-head-{index}"))
                    .w(140.0)
                    .px(10.0)
                    .py(8.0)
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
        });
    let mut table = col().key(TABLE_KEY).w_full().min_w(0.0).gap(0.0).child(header_row);
    for (index, column) in schema.columns.iter().enumerate() {
        let is_target = highlight.as_deref() == Some(column.name.as_str());
        let mut tr = row()
            .key(format!("struct-col-{}", column.name))
            .w_full()
            .min_w(0.0)
            .gap(0.0)
            .center_y()
            .fill(if is_target { k.c.surface_raised } else { k.c.layer_01 })
            .border(1.0, if is_target { k.c.border_focus } else { k.c.border_base });
        tr = tr.child(
            text(column.name.clone())
                .key(format!("struct-col-name-{}", column.name))
                .w(140.0)
                .px(10.0)
                .py(8.0)
                .font(k.ty.mono)
                .color(k.c.text_base),
        );
        tr = tr.child(
            text(column.data_type.clone())
                .key(format!("struct-col-type-{}", column.name))
                .w(140.0)
                .px(10.0)
                .py(8.0)
                .font(k.ty.mono)
                .color(k.c.text_base),
        );
        let nullable = if column.nullable { "YES" } else { "NO" };
        tr = tr.child(
            text(nullable.to_owned())
                .key(format!("struct-col-nullable-{}", column.name))
                .w(140.0)
                .px(10.0)
                .py(8.0)
                .font(k.ty.text_small)
                .color(if column.nullable { k.c.text_accent } else { k.c.text_muted }),
        );
        let default = match column.default_value.as_deref() {
            Some(value) if !value.is_empty() => value.to_owned(),
            _ => "—".to_owned(),
        };
        tr = tr.child(
            text(default)
                .key(format!("struct-col-default-{}", column.name))
                .w(140.0)
                .px(10.0)
                .py(8.0)
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );
        let mut badges = row().key(format!("struct-col-badges-{}", column.name)).w(140.0).px(10.0).py(8.0).gap(4.0);
        if pk.iter().any(|name| name == &column.name) {
            badges = badges.child(badge("PK", "struct-col-pk", k));
        }
        if unique.iter().any(|name| name == &column.name) {
            badges = badges.child(badge("UQ", "struct-col-uq", k));
        }
        if column.is_auto_increment {
            badges = badges.child(badge("AI", "struct-col-ai", k));
        }
        tr = tr.child(badges);
        tr = tr.child(
            text(column.comment.clone().unwrap_or_default())
                .key(format!("struct-col-comment-{}", column.name))
                .w(140.0)
                .px(10.0)
                .py(8.0)
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
        let _ = index;
        table = table.child(tr);
    }
    root = root.child(row().key("structure-view-table-scroll").w_full().grow(1.0).min_h(0.0).child(table));

    // ── Summary bar ─────────────────────────────────────────────────────────
    let summary = row()
        .key(SUMMARY_KEY)
        .w_full()
        .gap(14.0)
        .px(14.0)
        .py(10.0)
        .center_y()
        .fill(k.c.layer_02)
        .child(text(vm.fields_text()).key("structure-view-fields").font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(text(vm.primary_keys_text()).key("structure-view-pks").font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(text(vm.unique_indexes_text()).key("structure-view-uniques").font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(text(vm.index_count_text()).key("structure-view-indexes").font(k.ty.text_xsmall).color(k.c.text_muted))
        .child(text(vm.foreign_key_count_text()).key("structure-view-fks").font(k.ty.text_xsmall).color(k.c.text_muted));
    root.child(summary)
}

fn badge(label: &str, key: &str, k: &rune_kit::Kit) -> El {
    row()
        .key(key)
        .px(5.0)
        .py(2.0)
        .radius(4.0)
        .fill(k.c.layer_02)
        .child(text(label.to_owned()).font(k.ty.text_xsmall).color(k.c.text_accent))
}

fn loading_body(dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(LOADING_KEY)
        .w_full()
        .h_full()
        .center()
        .gap(8.0)
        .child(icon(&ICON_LOADER2, 18.0).color(k.c.text_muted))
        .child(text(t("common.loadingTableStructure")).font(k.ty.text_small).color(k.c.text_muted))
}

fn error_body<S: 'static>(error: &str, dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(ERROR_KEY)
        .w_full()
        .h_full()
        .center()
        .child(CopyableError::new("structure-view-error-copy", error.to_owned()).build::<S>(k))
}
