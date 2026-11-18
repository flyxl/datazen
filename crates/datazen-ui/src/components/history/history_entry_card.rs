//! One history row card.
//!
//! Ported from `packages/frontend/src/components/history/HistoryEntryCard.tsx`.
//! Structure follows React JSX 1:1: root card, header (meta left + actions right),
//! SQL block equivalent to React `<pre>` (single mono, wrap-anywhere, selectable text,
//! bounded by max height), optional error block.
//!
//! This component currently has no production consumer; it will be wired by the
//! query-history dialog/view track. Reusable API items are kept with per-item
//! `#[allow(dead_code)]` (not file-level) until wired.
//!
//! ## Keys
//!
//! - `history-entry-card-{id}`: root card
//! - `history-entry-card-{id}-header`: header row
//! - `history-entry-card-{id}-meta`: left meta row
//! - `history-entry-card-{id}-select`: select checkbox control
//! - `history-entry-card-{id}-status`: success/failure dot
//! - `history-entry-card-{id}-connection`: connection name
//! - `history-entry-card-{id}-database`: database/default-db label
//! - `history-entry-card-{id}-schema`: schema label (if present)
//! - `history-entry-card-{id}-time`: execution time
//! - `history-entry-card-{id}-rows`: rows affected (if present)
//! - `history-entry-card-{id}-executed-at`: executed timestamp
//! - `history-entry-card-{id}-actions`: actions row
//! - `history-entry-card-{id}-action-copy`: copy action
//! - `history-entry-card-{id}-action-favorite`: favorite action
//! - `history-entry-card-{id}-action-open`: open action (if can_open)
//! - `history-entry-card-{id}-action-delete`: delete action
//! - `history-entry-card-{id}-sql`: SQL block (single `<pre>`-equivalent text element)
//! - `history-entry-card-{id}-error`: error wrapper (if present)
//! - `history-entry-card-{id}-error-msg*`: CopyableError subtree

use std::rc::Rc;

use rune::{col, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

use crate::i18n::t;
use crate::icons::{ICON_EXTERNAL_LINK, ICON_STAR, ICON_TRASH};
use crate::ui::copyable::{BUTTON_SM, CopyButton as Copyable, CopyableError};
use crate::ui::toggle::Checkbox;

/// Action emitted by a history entry card control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) enum HistoryEntryAction {
    Copy,
    Open,
    Delete,
    Favorite,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl HistoryEntryAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Open => "open",
            Self::Delete => "delete",
            Self::Favorite => "favorite",
        }
    }
}

/// Minimal view model for one history entry row.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct HistoryEntryVm {
    pub id: String,
    pub success: bool,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub execution_time_ms: i64,
    pub rows_affected: Option<i64>,
    /// Preformatted display string (React uses `toLocaleString()`; caller provides display text).
    pub executed_at: String,
    pub sql: String,
    pub error_message: Option<String>,
}

type ToggleFn<S> = Box<dyn Fn(&mut S, String) + 'static>;
type ActionFn<S> = Box<dyn Fn(&mut S, HistoryEntryAction, String) + 'static>;
type CopyFn<S> = Box<dyn Fn(&mut S, String, &mut Tx) + 'static>;

/// Builder for a history entry card.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct HistoryEntryCard<S: 'static> {
    entry: HistoryEntryVm,
    connection_name: String,
    selected: bool,
    copied: bool,
    can_open: bool,
    on_toggle_select: Option<ToggleFn<S>>,
    on_action: Option<ActionFn<S>>,
    /// Called after copying SQL to clipboard + scheduling wake; receives entry id.
    on_copy: Option<CopyFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> HistoryEntryCard<S> {
    pub(crate) fn new(entry: HistoryEntryVm, connection_name: impl Into<String>) -> Self {
        Self {
            entry,
            connection_name: connection_name.into(),
            selected: false,
            copied: false,
            can_open: false,
            on_toggle_select: None,
            on_action: None,
            on_copy: None,
        }
    }

    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Whether copy feedback (`common.copied`) is active for this entry.
    pub(crate) fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    pub(crate) fn can_open(mut self, can_open: bool) -> Self {
        self.can_open = can_open;
        self
    }

    pub(crate) fn on_toggle_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_select = Some(Box::new(f));
        self
    }

    pub(crate) fn on_action(
        mut self,
        f: impl Fn(&mut S, HistoryEntryAction, String) + 'static,
    ) -> Self {
        self.on_action = Some(Box::new(f));
        self
    }

    /// Hook invoked after SQL is copied and feedback wake scheduled; use to record copied-until.
    pub(crate) fn on_copy(mut self, f: impl Fn(&mut S, String, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let HistoryEntryCard {
            entry,
            connection_name,
            selected,
            copied,
            can_open,
            on_toggle_select,
            on_action,
            on_copy,
        } = self;
        let on_action = on_action.map(Rc::new);

        let id = entry.id.clone();
        let root_key = format!("history-entry-card-{id}");

        let (border, fill) = if selected {
            (k.alpha(k.c.bg_accent, 0.6), k.alpha(k.c.bg_accent, 0.05))
        } else {
            (k.alpha(k.c.border_base, 0.8), k.c.layer_01)
        };

        // --- meta (left side of header) ---
        let mut meta = row()
            .key(format!("{root_key}-meta"))
            .items_center()
            .gap(8.0)
            .grow(1.0)
            .min_w(0.0);

        // select checkbox (kit base: Checkbox). Label kept for accessibility.
        let mut cb = Checkbox::new(format!("{root_key}-select"), selected)
            .label(t("query.historySelectAll"));
        if let Some(toggle) = on_toggle_select {
            let tid = id.clone();
            cb = cb.on_toggle(move |s: &mut S| toggle(s, tid.clone()));
        }
        meta = meta.child(cb.build(&k));

        // success/failure dot
        let dot_color = if entry.success {
            k.c.success.fg
        } else {
            k.c.danger.fg
        };
        meta = meta.child(
            row()
                .key(format!("{root_key}-status"))
                .size(6.0, 6.0)
                .shrink()
                .radius(3.0)
                .fill(dot_color),
        );

        // connection name
        meta = meta.child(
            text(connection_name)
                .key(format!("{root_key}-connection"))
                .font(k.ty.text_small_medium)
                .color(k.c.text_muted)
                .truncate()
                .max_w(180.0),
        );
        meta = meta.child(sep(&k, &root_key, "sep-conn"));

        // database or default
        let db_label = entry
            .database
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t("query.historyDefaultDb").to_string());
        meta = meta.child(
            text(db_label)
                .key(format!("{root_key}-database"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .truncate()
                .max_w(140.0),
        );

        if let Some(schema) = entry.schema.clone().filter(|s| !s.is_empty()) {
            meta = meta.child(sep(&k, &root_key, "sep-schema"));
            meta = meta.child(
                text(schema)
                    .key(format!("{root_key}-schema"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .truncate()
                    .max_w(120.0),
            );
        }

        meta = meta.child(sep(&k, &root_key, "sep-time"));
        meta = meta.child(
            text(format!("{}ms", entry.execution_time_ms))
                .key(format!("{root_key}-time"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );

        if let Some(rows) = entry.rows_affected {
            meta = meta.child(sep(&k, &root_key, "sep-rows"));
            let rows_label = t("query.historyRows").replace("{count}", &rows.to_string());
            meta = meta.child(
                text(rows_label)
                    .key(format!("{root_key}-rows"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            );
        }

        meta = meta.child(sep(&k, &root_key, "sep-date"));
        meta = meta.child(
            text(entry.executed_at.clone())
                .key(format!("{root_key}-executed-at"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );

        // --- actions (right side of header) ---
        let mut actions = row()
            .key(format!("{root_key}-actions"))
            .items_center()
            .gap(6.0)
            .shrink();

        // copy action (shared copyable base). Copy/feedback semantics live in
        // `CopyButton`: tx.copy + 1500ms feedback wake + caller hooks, with label/icon
        // driven by the `copied` prop. Styling/tokens come from the kit Button.
        {
            let sql = entry.sql.clone();
            let eid_action = id.clone();
            let eid_callback = id.clone();
            let action_sink = on_action.clone();
            let copy = Copyable::new(format!("{root_key}-action-copy"), &sql)
                .copied(copied)
                .label(t("query.historyCopy"))
                .help(t("query.historyCopyTitle"))
                .height(BUTTON_SM)
                .on_copy_tx(move |s: &mut S, tx: &mut Tx| {
                    if let Some(ref act) = action_sink {
                        act(s, HistoryEntryAction::Copy, eid_action.clone());
                    }
                    if let Some(ref cb) = on_copy {
                        cb(s, eid_callback.clone(), tx);
                    }
                });
            actions = actions.child(copy.build::<S>(dark));
        }

        // favorite action
        {
            let mut btn = Button::new(t("query.historyFavorite"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_STAR)
                .help(t("query.historyFavoriteTitle"));
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn = btn.on_press(move |s: &mut S| act(s, HistoryEntryAction::Favorite, eid.clone()));
            }
            actions = actions.child(btn.build(&k).key(format!("{root_key}-action-favorite")));
        }

        // open action (conditional)
        if can_open {
            let mut btn = Button::new(t("query.historyOpen"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .icon(&ICON_EXTERNAL_LINK)
                .help(t("query.historyOpenTitle"));
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn = btn.on_press(move |s: &mut S| act(s, HistoryEntryAction::Open, eid.clone()));
            }
            actions = actions.child(btn.build(&k).key(format!("{root_key}-action-open")));
        }

        // delete action (icon-only, kit IconButton base)
        {
            let mut btn = IconButton::new(&ICON_TRASH)
                .variant(IconButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .label(t("query.historyDeleteTitle"))
                .help(t("query.historyDeleteTitle"));
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn = btn.on_press(move |s: &mut S| act(s, HistoryEntryAction::Delete, eid.clone()));
            }
            actions = actions.child(btn.build(&k).key(format!("{root_key}-action-delete")));
        }

        let header = row()
            .key(format!("{root_key}-header"))
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(meta)
            .child(actions);

        // SQL block: single text element equivalent to React `<pre>` (mono, wrap-anywhere,
        // selectable), bounded by max height.
        let sql_block = text(entry.sql.clone())
            .key(format!("{root_key}-sql"))
            .w_full()
            .max_h(96.0)
            .min_w(0.0)
            .radius(4.0)
            .fill(k.alpha(k.c.bg_base, 0.7))
            .padding(8.0)
            .font(k.ty.mono)
            .color(k.c.text_muted)
            .wrap_anywhere()
            .selectable(true);

        let mut card = col()
            .key(root_key.clone())
            .w_full()
            .gap(6.0)
            .radius(8.0)
            .border(1.0, border)
            .fill(fill)
            .padding(12.0)
            .child(header)
            .child(sql_block);

        if let Some(err) = entry.error_message {
            let err_wrap = row()
                .key(format!("{root_key}-error"))
                .w_full()
                .radius(4.0)
                .fill(k.alpha(k.c.danger.fg, 0.10))
                .px(8.0)
                .py(4.0)
                .child(
                    CopyableError::new(format!("{root_key}-error-msg"), err)
                        .copy_button(true)
                        .build::<S>(&k),
                );
            card = card.child(err_wrap);
        }

        card
    }
}

fn sep(k: &Kit, root_key: &str, suffix: &str) -> El {
    text("·")
        .key(format!("{root_key}-{suffix}"))
        .font(k.ty.text_xsmall)
        .color(k.c.text_faint)
}
