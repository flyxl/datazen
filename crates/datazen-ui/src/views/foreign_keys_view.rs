//! Port of `packages/frontend/src/windows/connection/ForeignKeysView.tsx`.
//!
//! React loads a table's foreign keys with `getCachedTableSchema`, shows a
//! spinner while loading, a `CopyableError` on failure, a translated empty state
//! when there are none, and a read-only six-column table otherwise. This port
//! keeps that four-way split; the cached-schema lookup becomes an injected
//! [`FkAction::Load`] the host executes, with the answer fed back through
//! [`FkViewState::apply_load`]. No fake IPC, timer or thread.
//!
//! ## Local stand-ins (removal stage)
//!
//! * The React file has **no** `data-testid` attributes. Keys below are Rune
//!   observation handles named after the React elements; remove that distinction
//!   if the React side adds test ids, keeping the names.
//! * `ui/table.rs` renders one text value per cell, while React's local/reference
//!   columns are chip lists. The header reuses its `Column` definitions; the body
//!   is drawn with keyed rows/chips. Remove the manual body when the table base
//!   gains custom cell builders.
//! * `ForeignKeyInfo` is the shared relation-metadata DTO; no FK VM is duplicated.
//! * React's view has no navigation callback; an optional host port lets a Rune
//!   host open the referenced table. Remove it if the React view gains its own.
//!
//! Not composed into the content-view slot yet: every item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the host wires it".

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::relation_metadata::ForeignKeyInfo;
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::ui::table::Column;

pub(crate) const ROOT_KEY: &str = "foreign-keys-view";
pub(crate) const LOADING_KEY: &str = "foreign-keys-loading";
pub(crate) const ERROR_KEY: &str = "foreign-keys-error";
pub(crate) const EMPTY_KEY: &str = "foreign-keys-empty";
pub(crate) const HEADER_KEY: &str = "foreign-keys-header";
pub(crate) const TABLE_KEY: &str = "foreign-keys-table";

/// React props, kept as a contract mirror for the host.
#[allow(dead_code)] // contract mirror; remove when the host panel store lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkIdentity {
    pub(crate) db_session_id: String,
    pub(crate) table_name: String,
    pub(crate) database: String,
}

/// The three `useState` values React owns.
#[allow(dead_code)] // VM; owned by the host until the panel view lands.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct FkViewState {
    pub(crate) foreign_keys: Vec<ForeignKeyInfo>,
    pub(crate) loading: bool,
    pub(crate) error: String,
}

#[allow(dead_code)] // host port; remove when the host wires the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FkAction {
    Load,
}

#[allow(dead_code)] // state transitions; remove with the host.
impl FkViewState {
    /// React's effect prologue: start the spinner and clear the prior result.
    pub(crate) fn begin_load(&mut self) {
        self.loading = true;
        self.error.clear();
        self.foreign_keys.clear();
    }

    /// React's `.then` / `.catch`: any empty error message falls back to the
    /// translated load-failure sentence; both branches clear the spinner.
    pub(crate) fn apply_load(&mut self, result: Result<Vec<ForeignKeyInfo>, String>) {
        self.loading = false;
        match result {
            Ok(keys) => {
                self.foreign_keys = keys;
                self.error.clear();
            }
            Err(message) => {
                self.foreign_keys.clear();
                self.error = if message.is_empty() {
                    t("fk.loadFailed").to_owned()
                } else {
                    message
                };
            }
        }
    }
}

/// The six React header cells. `Column` comes from `ui/table.rs`; widths match
/// the React table's `w-full border-collapse` equal share (no fixed widths).
#[allow(dead_code)] // layout definitions; consumed by build.
fn header_columns() -> Vec<Column> {
    vec![
        Column::new(t("fk.constraintName")),
        Column::new(t("fk.localColumn")),
        Column::new(t("fk.refTable")),
        Column::new(t("fk.refColumn")),
        Column::new("ON UPDATE"),
        Column::new("ON DELETE"),
    ]
}

type ActionFn<S> = Rc<dyn Fn(&mut S, FkAction) + 'static>;
type OpenFn<S> = Rc<dyn Fn(&mut S, &str, &str) + 'static>;

/// Render the view. `on_action` is the host's cached-schema lookup trigger;
/// `on_open_table` optionally navigates to a referenced table.
#[allow(dead_code)] // view entry; remove when the host wires it.
pub(crate) fn build<S: 'static>(
    identity: &FkIdentity,
    state: &FkViewState,
    on_action: Option<ActionFn<S>>,
    on_open_table: Option<OpenFn<S>>,
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
                .child(Spinner::new("foreign-keys-spinner").size(SpinnerSize::Lg).build(dark))
                .child(
                    text(t("fk.loading"))
                        .key("foreign-keys-loading-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }
    if !state.error.is_empty() {
        // React uses `CopyableError` here: its message node is `role="alert"`
        // and selectable. Reuse the Rune port rather than a plain text node.
        layer.push(
            CopyableError::new(ERROR_KEY, state.error.clone())
                .build::<S>(k)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .padding(16.0),
        );
        return;
    }
    if state.foreign_keys.is_empty() {
        layer.push(
            col()
                .key(EMPTY_KEY)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .child(
                    text(t("fk.noForeignKeys"))
                        .key("foreign-keys-empty-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }

    let header = header_row(dark);
    let mut body = col().key("foreign-keys-body").w_full().min_w(0.0);
    for fk in &state.foreign_keys {
        body = body.child(fk_row(fk, &identity.database, on_open_table.clone(), dark));
    }
    let count = t("common.foreignKeyCount").replace("{count}", &state.foreign_keys.len().to_string());
    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .fill(k.c.surface_base)
        .child(
            row()
                .key(HEADER_KEY)
                .w_full()
                .gap(8.0)
                .center_y()
                .px(16.0)
                .py(12.0)
                .child(
                    text(&identity.table_name)
                        .key("foreign-keys-title")
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text(count)
                        .key("foreign-keys-count")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            col()
                .key(TABLE_KEY)
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0)
                .clip_soft(0.0)
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(header)
                .child(body),
        );
    if let Some(on_action) = on_action {
        root = root.on_press(move |s: &mut S| on_action(s, FkAction::Load));
    }
    layer.push(root);
}

fn header_row(dark: bool) -> El {
    let k = kit(dark);
    // Reuse `Column` titles from `ui/table.rs`; chips cannot be expressed by
    // `DataTable` cells, so the body is keyed rows (see module docs).
    let columns = header_columns();
    let mut head = row()
        .key("foreign-keys-head")
        .w_full()
        .min_w(0.0)
        .h(32.0)
        .center_y()
        .fill(k.c.layer_02);
    for (index, column) in columns.iter().enumerate() {
        head = head.child(
            text(column.title.clone())
                .key(format!("foreign-keys-head-{index}"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        );
    }
    head
}

fn chips(values: &[String], key_prefix: &str, accent: bool, dark: bool) -> El {
    let k = kit(dark);
    let mut wrap = row()
        .key(key_prefix.to_owned())
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .center_y();
    for (index, value) in values.iter().enumerate() {
        wrap = wrap.child(
            row()
                .key(format!("{key_prefix}-{index}"))
                .h(18.0)
                .px(6.0)
                .center_y()
                .radius(4.0)
                .fill(if accent { k.alpha(k.c.text_accent, 0.10) } else { k.alpha(k.c.warning.fg, 0.10) })
                .child(
                    text(value.clone())
                        .font(k.ty.text_xsmall)
                        .color(if accent { k.c.text_accent } else { k.c.warning.fg }),
                ),
        );
    }
    wrap
}

fn fk_row<S: 'static>(
    fk: &ForeignKeyInfo,
    database: &str,
    on_open_table: Option<OpenFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut referenced = row()
        .key(format!("foreign-keys-ref-table-{}", fk.name))
        .gap(6.0)
        .center_y()
        .child(
            text(&fk.referenced_table)
                .font(k.ty.text_small)
                .color(k.c.text_accent)
                .truncate(),
        );
    if let Some(on_open) = on_open_table {
        let table = fk.referenced_table.clone();
        let database = database.to_owned();
        referenced = referenced.on_press(move |s: &mut S| on_open(s, &table, &database));
    }
    row()
        .key(format!("foreign-keys-row-{}", fk.name))
        .w_full()
        .min_w(0.0)
        .h(40.0)
        .gap(8.0)
        .center_y()
        .fill(k.c.surface_base)
        .child(
            text(&fk.name)
                .key(format!("foreign-keys-name-{}", fk.name))
                .font(k.ty.text_small)
                .color(k.c.text_base)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(chips(&fk.columns, &format!("foreign-keys-local-{}", fk.name), false, dark).grow(1.0).min_w(0.0))
        .child(referenced.grow(1.0).min_w(0.0))
        .child(
            chips(
                &fk.referenced_columns,
                &format!("foreign-keys-ref-cols-{}", fk.name),
                true,
                dark,
            )
            .grow(1.0)
            .min_w(0.0),
        )
        .child(
            text(fk.on_update.clone().unwrap_or_default())
                .key(format!("foreign-keys-on-update-{}", fk.name))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            text(fk.on_delete.clone().unwrap_or_default())
                .key(format!("foreign-keys-on-delete-{}", fk.name))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0),
        )
}
