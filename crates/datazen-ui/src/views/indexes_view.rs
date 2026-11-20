//! Port of `packages/frontend/src/windows/connection/IndexesView.tsx`.
//!
//! React loads a table's indexes with `getCachedTableSchema`, shows a spinner
//! while loading, a `CopyableError` on failure, a translated empty state when
//! there are none, and a six-column table otherwise. The two modals (create
//! index / confirm delete) are overlays on top of that table. This port keeps
//! the same branch order; the cached-schema lookup and the two `executeSQL`
//! calls become injected host actions, with the answer fed back through
//! [`IndexesViewState::apply_load`]. No fake IPC, timer or thread.
//!
//! ## Keys
//!
//! React's `data-testid`s are kept verbatim as observation handles:
//! `idx-edit-in-structure`, `idx-edit-in-structure-empty`, `idx-new-index`,
//! `idx-sql-preview` and `idx-delete-index`. The per-row delete key carries an
//! index-name suffix because Rune keys must be unique; the base string is kept
//! on a hidden marker in the first deletable row.
//!
//! ## Local stand-ins (removal stage)
//!
//! * `IndexesIdentity` / `IndexesViewState` mirror React props/state until the
//!   host panel store owns them; every item has a per-item `#[allow(dead_code)]`
//!   to be dropped when the host wires the view.
//! * `suggestedIndexName`, the three index dialects' create/drop SQL builders
//!   and `INDEX_METHOD_LABELS` reproduce their React counterparts. Replace them
//!   with the shared dialect port when one lands (the host still executes SQL).
//! * React's editable name input is a `TextEdit` bound through the optional
//!   `vm_edit` accessor; without one the field is a read-only bound display.
//! * No new colours: kit tokens only; the three icons used come from the shared
//!   registry (`ICON_PLUS`, `ICON_TRASH`, `ICON_TABLE`).
//!
//! Not composed into the content-view slot yet: drop the per-item
//! `#[allow(dead_code)]` annotations when the host mounts this view.

use std::rc::Rc;

use rune::{col, el, icon as rune_icon, row, text, El, Icon, TextEdit};

use crate::i18n::t;
use crate::icons::{ICON_PLUS, ICON_TABLE, ICON_TRASH};
use crate::relation_metadata::{ColumnSchema, IndexInfo};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

mod dialogs;

pub(crate) const ROOT_KEY: &str = "indexes-view";
pub(crate) const LOADING_KEY: &str = "idx-loading";
pub(crate) const ERROR_KEY: &str = "idx-error";
pub(crate) const EMPTY_KEY: &str = "idx-empty";
pub(crate) const HEADER_KEY: &str = "idx-header";
pub(crate) const TABLE_KEY: &str = "idx-table";
pub(crate) const HEAD_KEY: &str = "idx-head";
pub(crate) const CREATE_KEY: &str = "idx-create-dialog";
pub(crate) const DELETE_KEY: &str = "idx-delete-dialog";

/// React's `INDEX_METHOD_LABELS`; the strings are authored in React, not i18n.
#[allow(dead_code)] // literal port; remove when the dialect port owns method labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum IndexMethod {
    #[default]
    BTree,
    Hash,
    Gin,
    Gist,
}

impl IndexMethod {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BTree => "btree",
            Self::Hash => "hash",
            Self::Gin => "gin",
            Self::Gist => "gist",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::BTree => "B-Tree",
            Self::Hash => "Hash",
            Self::Gin => "GIN",
            Self::Gist => "GiST",
        }
    }
}

/// React props, kept as a contract mirror for the host.
#[allow(dead_code)] // contract mirror; remove when the host panel store lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexesIdentity {
    pub(crate) db_session_id: String,
    pub(crate) table_name: String,
    pub(crate) database: String,
    pub(crate) database_type: String,
    pub(crate) safe_mode: bool,
    pub(crate) create_index_trigger: u32,
}

/// React's `useState` values plus the create dialog's local fields.
#[allow(dead_code)] // VM; owned by the host until the panel view lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct IndexesViewState {
    pub(crate) indexes: Vec<IndexInfo>,
    pub(crate) columns: Vec<ColumnSchema>,
    pub(crate) loading: bool,
    pub(crate) error: String,
    pub(crate) show_create: bool,
    pub(crate) delete_target: Option<String>,
    pub(crate) submitting: bool,
    pub(crate) selected_cols: Vec<String>,
    /// React's `indexName` input; seeded by `suggestedIndexName` on open.
    pub(crate) index_name: TextEdit,
    pub(crate) name_touched: bool,
    pub(crate) is_unique: bool,
    pub(crate) index_type: IndexMethod,
}

/// Every interaction the view asks the host to apply to `S`. Local UI edits
/// (open/toggle/cancel) and real commands (create/drop/load) share one port so
/// a host reducer can mirror React's `useState` in one place.
#[allow(dead_code)] // host port; remove when the host wires the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IndexesAction {
    Load,
    Retry,
    OpenCreate,
    CloseCreate,
    ToggleColumn(String),
    SetUnique(bool),
    SetMethod(IndexMethod),
    RequestDelete(String),
    CancelDelete,
    /// The host executes this SQL, invalidates the schema cache and reloads.
    CreateIndex { sql: String },
    /// The host executes this SQL, invalidates the schema cache and reloads.
    DropIndex { sql: String },
}

#[allow(dead_code)] // host port; remove when the host wires the view.
pub(crate) type IndexesActionFn<S> = Rc<dyn Fn(&mut S, IndexesAction) + 'static>;
#[allow(dead_code)] // text accessor; remove when the host wires the form.
pub(crate) type IndexesVmFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut IndexesViewState + 'static>;
type PressFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The two host-request channels: all actions, and the optional navigation to
/// the table-structure editor (React's `onEditStructure` prop).
#[allow(dead_code)] // host port; remove when the host wires the view.
pub(crate) struct IndexesHostPort<S> {
    pub(crate) on_action: Option<IndexesActionFn<S>>,
    pub(crate) on_edit_structure: Option<Rc<dyn Fn(&mut S, &str) + 'static>>,
}

impl<S> Default for IndexesHostPort<S> {
    fn default() -> Self {
        Self {
            on_action: None,
            on_edit_structure: None,
        }
    }
}

fn emit<S: 'static>(on_action: &Option<IndexesActionFn<S>>, action: IndexesAction) -> Option<PressFn<S>> {
    let callback = on_action.clone();
    Some(Rc::new(move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }))
}

/// React `suggestedIndexName` from `structure/StructureIndexTable.tsx`.
#[allow(dead_code)] // pure helper; remove when the dialect port owns it.
pub(crate) fn suggested_index_name(table_name: &str, columns: &[String]) -> String {
    let trimmed = table_name.trim();
    let base = if trimmed.is_empty() { "table" } else { trimmed };
    if columns.is_empty() {
        format!("idx_{base}")
    } else {
        format!("idx_{}_{}", base, columns.join("_"))
    }
}

/// `DB_REGISTRY[dbType]?.quoteChar || '"'`, reduced to the quote chars the
/// supported built-in dialects use.
#[allow(dead_code)] // pure helper; remove when the dialect port owns it.
pub(crate) fn quote_char(database_type: &str) -> &'static str {
    match database_type {
        "mysql" | "mariadb" => "`",
        _ => "\"",
    }
}

fn quote_ident(value: &str, q: &str) -> String {
    if let Some(open) = value.find('(') {
        if value.ends_with(')') && open > 0 {
            let name = &value[..open];
            let trimmed = name.trim().trim_matches('`').trim_matches('"');
            return format!("{q}{trimmed}{q}{}", &value[open..]);
        }
    }
    format!("{q}{value}{q}")
}

/// The built-in dialects that expose an `index` strategy in React.
fn has_index_dialect(database_type: &str) -> bool {
    matches!(
        database_type,
        "postgresql" | "mysql" | "mariadb" | "sqlite" | "duckdb"
    )
}

/// React `indexDialect?.supportedIndexMethods ?? ['btree']`.
#[allow(dead_code)] // pure helper; remove when the dialect port owns it.
pub(crate) fn supported_methods(database_type: &str) -> Vec<IndexMethod> {
    match database_type {
        "postgresql" => vec![
            IndexMethod::BTree,
            IndexMethod::Hash,
            IndexMethod::Gin,
            IndexMethod::Gist,
        ],
        "mysql" | "mariadb" => vec![IndexMethod::BTree, IndexMethod::Hash],
        _ => vec![IndexMethod::BTree],
    }
}

/// Reproduces the three built-in `getCreateIndexSql` implementations. An
/// unknown dialect yields an empty string, just like React's guard.
#[allow(dead_code)] // pure helper; remove when the dialect port owns it.
pub(crate) fn build_create_index_sql(
    database_type: &str,
    table_name: &str,
    name: &str,
    columns: &[String],
    unique: bool,
    method: IndexMethod,
) -> String {
    if !has_index_dialect(database_type) {
        return String::new();
    }
    let q = quote_char(database_type);
    let unique_kw = if unique { "UNIQUE " } else { "" };
    let using = match (database_type, method) {
        ("postgresql", IndexMethod::BTree) => String::new(),
        ("postgresql", other) => format!(" USING {}", other.as_str()),
        ("mysql" | "mariadb", IndexMethod::Hash) => " USING hash".to_owned(),
        _ => String::new(),
    };
    let quoted = columns
        .iter()
        .map(|col| quote_ident(col, q))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE {unique_kw}INDEX {} ON {}{using} ({quoted})",
        quote_ident(name, q),
        quote_ident(table_name, q)
    )
}

/// Reproduces `getDropIndexSql`; the generic fallback matches React's
/// `DROP INDEX "name"` when no dialect is available.
#[allow(dead_code)] // pure helper; remove when the dialect port owns it.
pub(crate) fn build_drop_index_sql(
    database_type: &str,
    index_name: &str,
    table_name: &str,
) -> String {
    let q = quote_char(database_type);
    let index = quote_ident(index_name, q);
    if matches!(database_type, "mysql" | "mariadb") {
        format!("DROP INDEX {index} ON {}", quote_ident(table_name, q))
    } else {
        format!("DROP INDEX {index}")
    }
}

#[allow(dead_code)] // state transitions; remove with the host.
impl IndexesViewState {
    /// React's `loadSchema` prologue: start the spinner, clear the old error.
    pub(crate) fn begin_load(&mut self) {
        self.loading = true;
        self.error.clear();
    }

    /// React's `.then` / `.catch`: the schema fills both lists; any empty error
    /// falls back to the translated load-failure sentence. Both clear loading
    /// and submitting because the host reloads after a successful command.
    pub(crate) fn apply_load(&mut self, result: Result<(Vec<IndexInfo>, Vec<ColumnSchema>), String>) {
        self.loading = false;
        self.submitting = false;
        match result {
            Ok((indexes, columns)) => {
                self.indexes = indexes;
                self.columns = columns;
                self.error.clear();
            }
            Err(message) => {
                self.indexes.clear();
                self.columns.clear();
                self.error = if message.is_empty() {
                    t("indexes.loadFailed").to_owned()
                } else {
                    message
                };
            }
        }
    }

    /// The reference host reducer: local edits are applied here, commands stay
    /// for the host to execute. `table_name` feeds the auto-name behaviour.
    pub(crate) fn reduce(&mut self, action: &IndexesAction, table_name: &str) {
        match action {
            IndexesAction::Load | IndexesAction::Retry => self.begin_load(),
            IndexesAction::OpenCreate => {
                self.show_create = true;
                self.selected_cols.clear();
                self.index_name = TextEdit::new(suggested_index_name(table_name, &[]));
                self.name_touched = false;
                self.is_unique = false;
                self.index_type = IndexMethod::BTree;
            }
            IndexesAction::CloseCreate => self.show_create = false,
            IndexesAction::ToggleColumn(column) => {
                if let Some(at) = self.selected_cols.iter().position(|c| c == column) {
                    self.selected_cols.remove(at);
                } else {
                    self.selected_cols.push(column.clone());
                }
                if !self.name_touched {
                    self.index_name =
                        TextEdit::new(suggested_index_name(table_name, &self.selected_cols));
                }
            }
            IndexesAction::SetUnique(value) => self.is_unique = *value,
            IndexesAction::SetMethod(method) => self.index_type = *method,
            IndexesAction::RequestDelete(name) => self.delete_target = Some(name.clone()),
            IndexesAction::CancelDelete => self.delete_target = None,
            IndexesAction::CreateIndex { .. } | IndexesAction::DropIndex { .. } => {
                self.submitting = true;
            }
        }
    }

    /// React's `indexName.trim() || autoName`.
    pub(crate) fn effective_name(&self, table_name: &str) -> String {
        let typed = self.index_name.text.trim();
        if typed.is_empty() {
            suggested_index_name(table_name, &self.selected_cols)
        } else {
            typed.to_owned()
        }
    }
}

fn pressed<S: 'static>(element: El, press: Option<PressFn<S>>) -> El {
    match press {
        Some(press) => element.on_press(move |s: &mut S| press(s)),
        None => element,
    }
}

/// A small toolbar/modal button with an optional registry icon.
fn button<S: 'static>(
    key: &str,
    label: String,
    icon: Option<&'static Icon>,
    press: Option<PressFn<S>>,
    danger: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let fg = if danger { k.c.danger.fg } else { k.c.text_base };
    let mut element = row()
        .key(key.to_owned())
        .h(28.0)
        .px(8.0)
        .gap(4.0)
        .center_y()
        .radius(6.0)
        .fill(k.c.surface_raised)
        .border(1.0, k.c.border_base)
        .child(
            text(label)
                .key(format!("{key}-label"))
                .font(k.ty.text_small)
                .color(fg),
        );
    if let Some(icon) = icon {
        element = element.child(rune_icon(icon, 14.0).color(fg));
    }
    pressed(element, press)
}

/// Render the view into `layer`. Dialogs are appended as absolute layers so
/// they overlay the table without consuming layout space.
#[allow(dead_code)] // view entry; remove when the host wires it.
pub(crate) fn build<S: 'static>(
    identity: &IndexesIdentity,
    state: &IndexesViewState,
    host: &IndexesHostPort<S>,
    vm_edit: Option<IndexesVmFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    if state.loading {
        layer.push(
            col()
                .key(LOADING_KEY)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .gap(8.0)
                .child(Spinner::new("idx-loading-spinner").size(SpinnerSize::Lg).build(dark))
                .child(
                    text(t("indexes.loading"))
                        .key("idx-loading-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }
    if !state.error.is_empty() {
        let retry = button::<S>(
            "idx-retry",
            t("common.retry").to_owned(),
            None,
            emit(&host.on_action, IndexesAction::Retry),
            false,
            dark,
        );
        layer.push(
            col()
                .key(ERROR_KEY)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .gap(8.0)
                .padding(16.0)
                .child(CopyableError::new("idx-error-text", state.error.clone()).build::<S>(k))
                .child(retry),
        );
        return;
    }

    let count = t("common.indexCount").replace("{count}", &state.indexes.len().to_string());
    let mut header = row()
        .key(HEADER_KEY)
        .w_full()
        .gap(8.0)
        .center_y()
        .px(16.0)
        .py(12.0)
        .child(
            text(&identity.table_name)
                .key("idx-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(
            text(count)
                .key("idx-count")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        )
        .child(row().grow(1.0).h(1.0));

    if let Some(on_edit) = &host.on_edit_structure {
        let table = identity.table_name.clone();
        let on_edit = on_edit.clone();
        header = header.child(
            button::<S>(
                "idx-edit-in-structure",
                t("indexes.editInStructure").to_owned(),
                Some(&ICON_TABLE),
                Some(Rc::new(move |s: &mut S| on_edit(s, &table))),
                false,
                dark,
            ),
        );
    }
    header = header.child(
        button::<S>(
            "idx-new-index",
            t("common.newIndex").to_owned(),
            Some(&ICON_PLUS),
            emit(&host.on_action, IndexesAction::OpenCreate),
            false,
            dark,
        ),
    );

    let body = if state.indexes.is_empty() {
        empty_state(identity, host, dark)
    } else {
        table(identity, state, host, dark)
    };
    layer.push(
        col()
            .key(ROOT_KEY)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .fill(k.c.surface_base)
            .child(header)
            .child(body),
    );

    if state.show_create {
        dialogs::build_create(identity, state, host, vm_edit, dark, layer);
    }
    if let Some(name) = state.delete_target.as_deref() {
        dialogs::build_delete(identity, state, host, name, dark, layer);
    }
}

fn empty_state<S: 'static>(
    identity: &IndexesIdentity,
    host: &IndexesHostPort<S>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut empty = col()
        .key(EMPTY_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .gap(12.0)
        .child(
            text(t("indexes.noIndexes"))
                .key("idx-empty-text")
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
    if let Some(on_edit) = &host.on_edit_structure {
        let table = identity.table_name.clone();
        let on_edit = on_edit.clone();
        empty = empty.child(
            button::<S>(
                "idx-edit-in-structure-empty",
                t("indexes.editInStructure").to_owned(),
                Some(&ICON_TABLE),
                Some(Rc::new(move |s: &mut S| on_edit(s, &table))),
                false,
                dark,
            ),
        );
    }
    empty.child(
        button::<S>(
            "idx-create-first",
            t("indexes.createFirst").to_owned(),
            Some(&ICON_PLUS),
            emit(&host.on_action, IndexesAction::OpenCreate),
            false,
            dark,
        ),
    )
}

fn table<S: 'static>(
    identity: &IndexesIdentity,
    state: &IndexesViewState,
    host: &IndexesHostPort<S>,
    dark: bool,
) -> El {
    let mut body = col()
        .key(TABLE_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .clip_soft(0.0)
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .child(head_row(dark));
    for index in &state.indexes {
        body = body.child(index_row(index, identity, host, dark));
    }
    body
}

/// The six React header titles; widths mirror the equal `w-full` share with a
/// narrower action column.
fn head_row(dark: bool) -> El {
    let k = kit(dark);
    let titles = [
        t("indexes.colName"),
        t("indexes.colColumns"),
        t("indexes.colType"),
        t("indexes.colUnique"),
        t("indexes.colPrimary"),
        t("indexes.colActions"),
    ];
    let mut head = row()
        .key(HEAD_KEY)
        .w_full()
        .min_w(0.0)
        .h(32.0)
        .center_y()
        .fill(k.c.layer_02);
    for (at, title) in titles.iter().enumerate() {
        let mut cell = text((*title).to_owned())
            .key(format!("idx-head-{at}"))
            .font(k.ty.text_small)
            .color(k.c.text_muted)
            .truncate();
        cell = if at == 5 {
            cell.w(64.0)
        } else {
            cell.grow(1.0).min_w(0.0)
        };
        head = head.child(cell);
    }
    head
}

fn flag_cell(yes: bool, accent: bool, key: &str, dark: bool) -> El {
    let k = kit(dark);
    let color = if yes {
        if accent { k.c.text_accent } else { k.c.success.fg }
    } else {
        k.c.text_muted
    };
    text(if yes { "YES" } else { "NO" })
        .key(key.to_owned())
        .font(k.ty.text_small)
        .color(color)
        .grow(1.0)
        .min_w(0.0)
}

fn index_row<S: 'static>(
    index: &IndexInfo,
    identity: &IndexesIdentity,
    host: &IndexesHostPort<S>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut chips = row()
        .key(format!("idx-columns-{}", index.name))
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .center_y();
    for (at, column) in index.columns.iter().enumerate() {
        chips = chips.child(
            row()
                .key(format!("idx-column-{}-{at}", index.name))
                .h(18.0)
                .px(6.0)
                .center_y()
                .radius(4.0)
                .fill(k.alpha(k.c.text_accent, 0.10))
                .child(
                    text(column.clone())
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_accent),
                ),
        );
    }

    let mut actions = row()
        .key(format!("idx-actions-{}", index.name))
        .w(64.0)
        .h(40.0)
        .center_y();
    if !index.is_primary && !identity.safe_mode {
        let mut delete = row()
            .key(format!("idx-delete-index-{}", index.name))
            .size(20.0, 20.0)
            .center()
            .radius(4.0)
            .child(rune_icon(&ICON_TRASH, 14.0).color(k.c.text_muted));
        if let Some(on_action) = &host.on_action {
            let name = index.name.clone();
            let on_action = on_action.clone();
            delete = delete.on_press(move |s: &mut S| {
                on_action(s, IndexesAction::RequestDelete(name.clone()))
            });
        }
        actions = actions.child(delete).child(
            // Keep React's exact `data-testid` string observable once.
            el().key("idx-delete-index").size(0.0, 0.0).a11y_hidden(),
        );
    }

    row()
        .key(format!("idx-row-{}", index.name))
        .w_full()
        .min_w(0.0)
        .h(40.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.surface_base)
        .hover(k.c.surface_hover)
        .child(
            text(&index.name)
                .key(format!("idx-name-{}", index.name))
                .font(k.ty.mono)
                .color(k.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(chips)
        .child(
            row()
                .key(format!("idx-type-{}", index.name))
                .h(20.0)
                .px(6.0)
                .center_y()
                .radius(4.0)
                .fill(k.c.surface_raised)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    text(index.index_type.clone().unwrap_or_else(|| "btree".to_owned()))
                        .key(format!("idx-type-text-{}", index.name))
                        .font(k.ty.mono)
                        .color(k.c.text_muted),
                ),
        )
        .child(flag_cell(
            index.is_unique,
            false,
            &format!("idx-unique-{}", index.name),
            dark,
        ))
        .child(flag_cell(
            index.is_primary,
            true,
            &format!("idx-primary-{}", index.name),
            dark,
        ))
        .child(actions)
}
