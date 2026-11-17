//! One history row card.
//!
//! Ported from `packages/frontend/src/components/history/HistoryEntryCard.tsx`.
//! Structure follows React JSX 1:1: root card, header (meta left + actions right),
//! SQL `<pre>` block, optional error block.
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
//! - `history-entry-card-{id}-sql`: SQL block
//! - `history-entry-card-{id}-error`: error wrapper (if present)
//! - `history-entry-card-{id}-error-msg*`: CopyableError subtree

use std::rc::Rc;
use std::time::Duration;

use rune::{col, icon, row, text, Color, El, Tx};

use crate::i18n::t;
use crate::icons::ICON_COPY;
use crate::ui::copyable::CopyableError;

/// Action emitted by a history entry card control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryEntryAction {
    Copy,
    Open,
    Delete,
    Favorite,
}

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

    /// Whether copy feedback (`query.historyCopied`) is active for this entry.
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

        // select checkbox
        let mut cb_el = row()
            .key(format!("{root_key}-select"))
            .focusable(true)
            .size(14.0, 14.0)
            .shrink()
            .center()
            .radius(3.0)
            .border(
                1.0,
                if selected {
                    k.c.bg_accent
                } else {
                    k.c.border_base
                },
            )
            .fill(if selected {
                k.c.bg_accent
            } else {
                Color::TRANSPARENT
            })
            .label(t("query.historySelectAll"));
        if selected {
            cb_el = cb_el.child(
                icon(k.glyph.check, 10.0)
                    .color(k.c.text_inverse)
                    .a11y_hidden(),
            );
        }
        if let Some(toggle) = on_toggle_select {
            let tid = id.clone();
            cb_el = cb_el.on_press(move |s: &mut S| toggle(s, tid.clone()));
        }
        meta = meta.child(cb_el);

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
        meta = meta.child(sep(k, &root_key, "sep-conn"));

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
            meta = meta.child(sep(k, &root_key, "sep-schema"));
            meta = meta.child(
                text(schema)
                    .key(format!("{root_key}-schema"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint)
                    .truncate()
                    .max_w(120.0),
            );
        }

        meta = meta.child(sep(k, &root_key, "sep-time"));
        meta = meta.child(
            text(format!("{}ms", entry.execution_time_ms))
                .key(format!("{root_key}-time"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint),
        );

        if let Some(rows) = entry.rows_affected {
            meta = meta.child(sep(k, &root_key, "sep-rows"));
            let rows_label = t("query.historyRows").replace("{count}", &rows.to_string());
            meta = meta.child(
                text(rows_label)
                    .key(format!("{root_key}-rows"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_faint),
            );
        }

        meta = meta.child(sep(k, &root_key, "sep-date"));
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

        // copy action
        {
            let label_text = if copied {
                t("common.copied").to_string()
            } else {
                t("query.historyCopy").to_string()
            };
            let mut btn = action_base(
                k,
                format!("{root_key}-action-copy"),
                label_text.clone(),
                t("query.historyCopyTitle").to_string(),
            );
            // icon: Copy (ICON_COPY); copied state in React shows Check icon + success color.
            // Check not in ICON_* registry (gap reported); keep copy icon and use success color/label for feedback.
            btn = btn.child(
                icon(&ICON_COPY, 12.0)
                    .color(if copied {
                        k.c.success.fg
                    } else {
                        k.c.text_faint
                    })
                    .a11y_hidden(),
            );
            btn = btn.child(text(label_text).font(k.ty.small).color(if copied {
                k.c.success.fg
            } else {
                k.c.text_faint
            }));
            let sql = entry.sql.clone();
            let eid = id.clone();
            btn = btn.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                tx.copy(sql.clone());
                tx.wake_after(Duration::from_millis(1500));
                if let Some(ref cb) = on_copy {
                    cb(s, eid.clone(), tx);
                }
            });
            actions = actions.child(btn);
        }

        // favorite action (no on_copy involvement; dispatches action)
        {
            let label_text = t("query.historyFavorite").to_string();
            let mut btn = action_base(
                k,
                format!("{root_key}-action-favorite"),
                label_text.clone(),
                t("query.historyFavoriteTitle").to_string(),
            );
            // icon Star missing from ICON_* registry (gap reported)
            btn = btn.child(text(label_text).font(k.ty.small).color(k.c.text_faint));
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn = btn
                    .on_press(move |s: &mut S| act(s, HistoryEntryAction::Favorite, eid.clone()));
            }
            actions = actions.child(btn);
        }

        // open action (conditional)
        if can_open {
            let label_text = t("query.historyOpen").to_string();
            let mut btn = action_base(
                k,
                format!("{root_key}-action-open"),
                label_text.clone(),
                t("query.historyOpenTitle").to_string(),
            );
            // icon ExternalLink missing from ICON_* registry (gap reported)
            btn = btn.child(text(label_text).font(k.ty.small).color(k.c.text_faint));
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn = btn.on_press(move |s: &mut S| act(s, HistoryEntryAction::Open, eid.clone()));
            }
            actions = actions.child(btn);
        }

        // delete action (icon-only in React; Trash2 missing - gap reported)
        {
            let mut btn = action_base(
                k,
                format!("{root_key}-action-delete"),
                String::new(),
                t("query.historyDeleteTitle").to_string(),
            );
            // no visible label in React; keep accessible label/title only
            if let Some(act) = &on_action {
                let act = act.clone();
                let eid = id.clone();
                btn =
                    btn.on_press(move |s: &mut S| act(s, HistoryEntryAction::Delete, eid.clone()));
            }
            actions = actions.child(btn);
        }

        let header = row()
            .key(format!("{root_key}-header"))
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(meta)
            .child(actions);

        // SQL block: max-h-24, overflow-x-auto, rounded, bg-surface/70, p-2, mono, xs, pre-wrap break-all
        let sql_block = col()
            .key(format!("{root_key}-sql"))
            .w_full()
            .max_h(96.0)
            .min_w(0.0)
            .radius(4.0)
            .fill(k.alpha(k.c.bg_base, 0.7))
            .padding(8.0)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            })
            .child(
                text(entry.sql.clone())
                    .key(format!("{root_key}-sql-text"))
                    .font(k.ty.mono)
                    .color(k.c.text_muted)
                    .wrap_anywhere()
                    .selectable(true),
            );

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
                        .build::<S>(k),
                );
            card = card.child(err_wrap);
        }

        card
    }
}

fn sep(k: &rune_kit::Kit, root_key: &str, suffix: &str) -> El {
    text("·")
        .key(format!("{root_key}-{suffix}"))
        .font(k.ty.text_xsmall)
        .color(k.c.text_faint)
}

fn action_base(k: &rune_kit::Kit, key: String, label: String, title: String) -> El {
    let accessible = if label.is_empty() { title } else { label };
    row()
        .key(key)
        .focusable(true)
        .items_center()
        .gap(4.0)
        .radius(4.0)
        .px(8.0)
        .py(2.0)
        .hover(k.c.hover)
        .label(accessible)
}
