//! The chrome half of the T112 `DashboardPanel` port: the board tab strip
//! with its add / import / docs actions (React lines 533–602), the toolbar
//! with the rename field and the four right-aligned icon buttons (lines
//! 604–677), and the two delete confirmations (lines 709–761). The keys are
//! the parent module's documented table; the geometry constants live here.

use std::rc::Rc;

use rune::{col, icon, row, text, El, KeyEvent, Key, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{
    ICON_BOOK_OPEN, ICON_DOWNLOAD, ICON_PENCIL, ICON_PLUS, ICON_REFRESH, ICON_TRASH, ICON_UPLOAD,
};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;

use super::{type_xs, DashboardPanel};

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React tab `gap-1.5 px-3 py-2 text-xs`; the name's `max-w-[120px]`.
const TAB_GAP: f32 = 6.0;
const TAB_PX: f32 = 12.0;
const TAB_PY: f32 = 8.0;
const TAB_NAME_MAX_W: f32 = 120.0;
/// React tab underline `h-0.5`.
const TAB_UNDERLINE_H: f32 = 2.0;
/// React tab-bar actions `gap-0.5 px-1` and the glyph `h-3.5 w-3.5`.
const TAB_ACTIONS_GAP: f32 = 2.0;
const TAB_ACTIONS_PX: f32 = 4.0;
/// React toolbar `gap-1 px-3 py-1.5`; the icon buttons `h-7 w-7`.
const TOOLBAR_GAP: f32 = 4.0;
const TOOLBAR_PX: f32 = 12.0;
const TOOLBAR_PY: f32 = 6.0;
const TOOLBAR_BUTTON: f32 = 28.0;
/// React toolbar glyphs `h-3.5 w-3.5` (the pause / refresh / export / delete
/// icons).
const TOOLBAR_ICON: f32 = 14.0;
/// React rename input `h-7 w-48 text-xs`.
const RENAME_INPUT_W: f32 = 192.0;
const RENAME_INPUT_H: f32 = 28.0;
/// React rename trigger `gap-1.5 rounded px-1.5 py-1 text-xs font-medium`,
/// the pencil `h-3 w-3`.
const RENAME_GAP: f32 = 6.0;
const RENAME_PX: f32 = 6.0;
const RENAME_PY: f32 = 4.0;
const RENAME_PENCIL: f32 = 12.0;
/// React dialog footer buttons `h-8 px-3 text-xs` (the `Large` kit tier, the
/// T018 mapping) and the body line `text-sm text-fg-secondary`.
const DIALOG_BODY: TextStyle = TextStyle::new(14.0, 400.0);

impl<'a, S: 'static> DashboardPanel<'a, S> {
    /// The tab bar (lines 533–602): the bordered strip row, the scrollable
    /// tab run, and the trailing actions.
    pub(super) fn tab_bar(&self, k: &Kit) -> El {
        let state = self.state;
        let mut strip = row()
            .key("dashboard-tab-bar")
            // React: `scrollbar-hide flex min-w-0 flex-1 overflow-x-auto` —
            // the horizontal viewport; the wheel already moves it, which is
            // the redirect React's `onWheel` hand-rolls (the T174 ruling).
            .grow(1.0)
            .min_w(0.0)
            .scroll_x();
        for board in &state.list {
            strip = strip.child(self.tab_entry(k, board));
        }
        row()
            .key("dashboard-tab-bar-row")
            .w_full()
            .shrink()
            .items_center()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base) // border-b
            .fill(k.c.layer_01) // bg-surface-alt (the T141 ruling)
            .child(strip)
            .child(self.tab_actions(k))
    }

    /// One board tab (lines 544–569): the whole button — name plus the
    /// always-present accent underline (transparent while inactive, the
    /// T174 in-flow ruling).
    fn tab_entry(&self, k: &Kit, board: &super::BoardListItemVm) -> El {
        let active = board.id == self.state.active_dashboard_id;
        let entry_key = format!("dashboard-tab-{}", board.id);
        let content = row()
            .key(format!("{entry_key}-content"))
            .items_center()
            .gap(TAB_GAP) // gap-1.5
            .px(TAB_PX) // px-3
            .py(TAB_PY) // py-2
            .child(
                text(board.name.clone())
                    .key(format!("{entry_key}-name"))
                    .font(TextStyle::new(12.0, if active { 500.0 } else { 400.0 })) // text-xs (+ font-medium)
                    .color(if active {
                        k.c.text_base // text-fg
                    } else {
                        k.c.text_muted // text-fg-secondary (the area ruling)
                    })
                    .max_w(TAB_NAME_MAX_W) // max-w-[120px]
                    .truncate(),
            );
        let underline = row()
            .key(format!("{entry_key}-underline"))
            .w_full()
            .h(TAB_UNDERLINE_H) // h-0.5
            .shrink()
            .fill(if active {
                k.c.bg_accent // bg-accent
            } else {
                rune::Color::TRANSPARENT // opacity-0 (the recoloring rule)
            });
        let mut entry = col()
            .key(entry_key.clone())
            .shrink()
            .role("button")
            .label(board.name.clone())
            // React: `border-r border-edge` — the right edge only.
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
            .child(content)
            .child(underline);
        if active {
            // React: `bg-surface font-medium text-fg`.
            entry = entry.fill(k.c.bg_base);
        } else {
            // React: `hover:bg-surface-raised hover:text-fg` — the standard
            // hover overlay (`transition-colors` has no counterpart).
            entry = entry.hover(k.c.surface_raised);
        }
        entry = entry.focusable(true).focus_ring(2.0, 2.0, k.c.border_focus);
        // The press: switchDashboard — the state half through the
        // projection, the `onDashboardChange` notification through the slot.
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            let id = board.id.clone();
            let notify = self.on_dashboard_change.clone();
            entry = entry.on_press(move |s: &mut S| {
                let pair = state_fn(s).switch_dashboard(&id);
                if let Some(notify) = &notify {
                    if let Some((id, name)) = pair {
                        notify(s, &id, &name);
                    }
                }
            });
        }
        entry
    }

    /// The tab bar's trailing actions (lines 571–600): add, import, docs.
    fn tab_actions(&self, k: &Kit) -> El {
        let state = self.state;
        let mut actions = row()
            .key("dashboard-tab-bar-actions")
            .shrink()
            .items_center()
            .gap(TAB_ACTIONS_GAP) // gap-0.5
            .px(TAB_ACTIONS_PX); // px-1

        // The add button (a new panel).
        let mut add =
            self.icon_button(k, "dashboard-tab-add", t("dashboard.newPanel"), &ICON_PLUS, None);
        if let Some(create_panel) = &self.on_create_panel {
            let create_panel = Rc::clone(create_panel);
            add = add.on_press(move |s: &mut S| create_panel(s));
        }
        actions = actions.child(add);

        // The import button (disabled while the board loads).
        if state.loading {
            actions = actions.child(self.icon_button(
                k,
                "dashboard-import",
                t("dashboard.import"),
                &ICON_UPLOAD,
                None,
            ));
        } else {
            let mut import = self.icon_button(
                k,
                "dashboard-import",
                t("dashboard.import"),
                &ICON_UPLOAD,
                None,
            );
            if let Some(do_import) = &self.on_import {
                let do_import = Rc::clone(do_import);
                import = import.on_press(move |s: &mut S| do_import(s));
            }
            actions = actions.child(import);
        }

        // The docs button.
        let mut docs = self.icon_button(
            k,
            "dashboard-docs-help",
            t("docs.openDashboardHelp"),
            &ICON_BOOK_OPEN,
            None,
        );
        if let Some(open_docs) = &self.on_open_docs {
            let open_docs = Rc::clone(open_docs);
            docs = docs.on_press(move |s: &mut S| open_docs(s));
        }
        actions = actions.child(docs);
        actions
    }

    /// The toolbar (lines 604–677): the rename half and the action run.
    /// `rename_edit` is the host's `on_edit` projection for the field (the
    /// T018 `name_edit` convention, taken from the builder).
    pub(super) fn toolbar(
        &self,
        k: &Kit,
        dark: bool,
        rename_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
    ) -> El {
        row()
            .key("dashboard-panel-toolbar")
            .w_full()
            .shrink()
            .items_center()
            .gap(TOOLBAR_GAP) // gap-1
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base) // border-b
            .fill(k.c.bg_base) // bg-surface
            .px(TOOLBAR_PX) // px-3
            .py(TOOLBAR_PY) // py-1.5
            .child(
                row()
                    .key("dashboard-panel-toolbar-title")
                    .grow(1.0) // flex-1
                    .min_w(0.0) // min-w-0
                    .items_center()
                    .child(self.rename_control(k, dark, rename_edit)),
            )
            .child(self.toolbar_actions(k, dark))
    }

    /// The rename control: the field while renaming, the trigger button
    /// otherwise (lines 608–632).
    fn rename_control(
        &self,
        k: &Kit,
        dark: bool,
        rename_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
    ) -> El {
        let state = self.state;
        if state.renaming {
            let mut field = TextField::new("dashboard-rename-input", &state.rename_edit)
                .width(RENAME_INPUT_W) // w-48
                .height(RENAME_INPUT_H) // h-7
                .text_style(type_xs()); // text-xs
            if let Some(edit) = rename_edit {
                field = edit(field);
            } else if let Some(state_fn) = &self.state_fn {
                // The unwired-host fallback: bind the field to the panel's
                // own TextEdit through the projection.
                let state_fn = Rc::clone(state_fn);
                field = field.on_edit(move |s: &mut S| &mut state_fn(s).rename_edit);
            }
            // React's `onKeyDown`: Enter commits, Escape cancels. The blur
            // commit has no Rune counterpart (a documented delta).
            if let Some(state_fn) = &self.state_fn {
                let state_fn = Rc::clone(state_fn);
                let commit = self.on_rename_commit.clone();
                field = field.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut rune::Tx| {
                    match ev.key {
                        Key::Enter => {
                            if let Some(commit) = &commit {
                                commit(s);
                            } else {
                                state_fn(s).commit_rename();
                            }
                            true
                        }
                        Key::Escape => {
                            state_fn(s).cancel_rename();
                            true
                        }
                        _ => false,
                    }
                });
            }
            return field.build(dark);
        }
        let current_name = state
            .current
            .as_ref()
            .map(|board| board.name.clone())
            .unwrap_or_default();
        let mut trigger = row()
            .key("dashboard-rename")
            .role("button")
            .label(t("dashboard.name"))
            .help(t("dashboard.name")) // React title
            .items_center()
            .gap(RENAME_GAP) // gap-1.5
            .px(RENAME_PX) // px-1.5
            .py(RENAME_PY) // py-1
            .radius(k.radius.sm) // rounded
            .min_w(0.0)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover(k.c.surface_raised) // hover:bg-surface-raised
            .child(
                text(current_name)
                    .key("dashboard-rename-name")
                    .font(TextStyle::new(12.0, 500.0)) // text-xs font-medium
                    .color(k.c.text_base) // text-fg
                    .truncate(),
            )
            .child(
                icon(&ICON_PENCIL, RENAME_PENCIL) // h-3 w-3
                    .key("dashboard-rename-icon")
                    .shrink()
                    .color(k.c.text_muted), // text-fg-muted
            );
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            trigger = trigger.on_press(move |s: &mut S| state_fn(s).begin_rename());
        }
        trigger
    }

    /// The toolbar's action run (lines 634–675).
    fn toolbar_actions(&self, k: &Kit, dark: bool) -> El {
        let state = self.state;
        let mut actions = row()
            .key("dashboard-panel-toolbar-actions")
            .items_center()
            .gap(TAB_ACTIONS_GAP); // gap-0.5

        // The monitor pause toggle: Pause while running, Play while paused.
        let pause_title = if state.monitor_paused {
            t("dashboard.resumeMonitoring")
        } else {
            t("dashboard.pauseMonitoring")
        };
        let mut pause = self.icon_button(
            k,
            "dashboard-pause-toggle",
            pause_title,
            super::monitor_toggle_glyph(state.monitor_paused),
            None,
        );
        if let Some(toggle) = &self.on_toggle_monitor_pause {
            let toggle = Rc::clone(toggle);
            let next = state.next_monitor_pause();
            pause = pause.on_press(move |s: &mut S| toggle(s, next));
        }
        actions = actions.child(pause);

        // Refresh-all: the spinner replaces the glyph while running.
        if state.refreshing_all {
            actions = actions.child(
                row()
                    .key("dashboard-refresh-all")
                    .size(TOOLBAR_BUTTON, TOOLBAR_BUTTON)
                    .center()
                    .radius(k.radius.md)
                    .child(
                        crate::ui::spinner::Spinner::new("dashboard-refresh-all-spinner")
                            .size(crate::ui::spinner::SpinnerSize::Md) // React size="md"
                            .build(dark),
                    ),
            );
        } else {
            let mut refresh =
                self.icon_button(k, "dashboard-refresh-all", "", &ICON_REFRESH, None);
            if let Some(refresh_all) = &self.on_refresh_all {
                let refresh_all = Rc::clone(refresh_all);
                let state_fn = self.state_fn.clone();
                refresh = refresh.on_press(move |s: &mut S| {
                    if let Some(state_fn) = &state_fn {
                        state_fn(s).begin_refresh_all();
                    }
                    refresh_all(s);
                });
            }
            actions = actions.child(refresh);
        }

        // Export (disabled while the board loads).
        if state.loading {
            actions = actions.child(self.icon_button(
                k,
                "dashboard-export",
                t("dashboard.export"),
                &ICON_DOWNLOAD,
                None,
            ));
        } else {
            let mut export =
                self.icon_button(k, "dashboard-export", t("dashboard.export"), &ICON_DOWNLOAD, None);
            if let Some(do_export) = &self.on_export {
                let do_export = Rc::clone(do_export);
                export = export.on_press(move |s: &mut S| do_export(s));
            }
            actions = actions.child(export);
        }

        // Delete panel (the danger ink; disabled while the board loads).
        let danger = k.c.danger.fg; // React text-red-400
        if state.loading {
            actions = actions.child(self.icon_button(
                k,
                "dashboard-delete-panel",
                t("dashboard.deletePanel"),
                &ICON_TRASH,
                Some(danger),
            ));
        } else {
            let mut delete = self.icon_button(
                k,
                "dashboard-delete-panel",
                t("dashboard.deletePanel"),
                &ICON_TRASH,
                Some(danger),
            );
            if let Some(state_fn) = &self.state_fn {
                let state_fn = Rc::clone(state_fn);
                delete = delete.on_press(move |s: &mut S| state_fn(s).open_delete_confirm());
            }
            actions = actions.child(delete);
        }
        actions
    }

    /// A 28×28 ghost icon button (React `variant="ghost" h-7 w-7 !px-0` —
    /// the T069 `plain_button` shape at the toolbar's size). `disabled`
    /// callers pass no press and get the inert control.
    fn icon_button(
        &self,
        k: &Kit,
        key: &str,
        title: &str,
        glyph: &'static rune::Icon,
        ink: Option<rune::Color>,
    ) -> El {
        let mut button = row()
            .key(key)
            .role("button")
            .label(title)
            .help(title) // React title
            .size(TOOLBAR_BUTTON, TOOLBAR_BUTTON) // h-7 w-7
            .center()
            .radius(k.radius.md)
            .child(
                icon(glyph, TOOLBAR_ICON) // h-3.5 w-3.5
                    .key(format!("{key}-icon"))
                    .color(ink.unwrap_or(k.c.text_muted)),
            );
        if ink.is_none() {
            // The ghost tint: focus and hover only on the non-danger buttons
            // (React's red pair keeps its static ink).
            button = button
                .focusable(true)
                .focus_ring(2.0, 2.0, k.c.border_focus)
                .hover_group()
                .hover(k.c.hover);
        }
        button
    }

    /// The two delete confirmations (lines 709–761), pushed on the layer in
    /// React's DOM order (the panel dialog first, the widget dialog second).
    pub(super) fn delete_dialogs(&self, k: &Kit, layer: &mut Vec<El>) {
        self.panel_delete_dialog(k, layer);
        self.widget_delete_dialog(k, layer);
    }

    /// The panel-delete dialog.
    fn panel_delete_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let mut dialog = Dialog::new("dashboard-panel-delete-dialog", t("dashboard.deletePanel"))
            .open(self.state.delete_confirm_open)
            .fit()
            .child(
                text(t("dashboard.deletePanelConfirm"))
                    .key("dashboard-panel-delete-dialog-text")
                    .font(DIALOG_BODY) // text-sm
                    .color(k.c.text_muted), // text-fg-secondary
            );
        // Footer: cancel (ghost), confirm (danger).
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large); // React h-8 px-3 text-xs
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            cancel = cancel.on_press(move |s: &mut S| state_fn(s).close_delete_confirm());
        }
        dialog = dialog.footer(cancel.build(k).key("dashboard-panel-delete-dialog-cancel"));

        let mut confirm = Button::new(t("common.delete"))
            .variant(ButtonVariant::Danger)
            .control_size(ControlSize::Large);
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            let delete_panel = self.on_delete_panel.clone();
            confirm = confirm.on_press(move |s: &mut S| {
                state_fn(s).begin_delete_panel();
                if let Some(delete_panel) = &delete_panel {
                    delete_panel(s);
                }
            });
        }
        dialog = dialog.footer(confirm.build(k).key("dashboard-delete-confirm"));

        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            dialog = dialog.on_close(move |s: &mut S| state_fn(s).close_delete_confirm());
        }
        dialog.build(k, layer);
    }

    /// The widget-delete dialog.
    fn widget_delete_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let mut dialog = Dialog::new(
            "dashboard-panel-widget-delete-dialog",
            t("dashboard.deleteWidget"),
        )
        .open(self.state.widget_to_delete.is_some())
        .fit()
        .child(
            text(t("dashboard.deleteWidgetConfirm"))
                .key("dashboard-panel-widget-delete-dialog-text")
                .font(DIALOG_BODY) // text-sm
                .color(k.c.text_muted), // text-fg-secondary
        );
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large);
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            cancel = cancel.on_press(move |s: &mut S| state_fn(s).cancel_delete_widget());
        }
        dialog = dialog.footer(cancel.build(k).key("dashboard-panel-widget-delete-dialog-cancel"));

        // React's literal testid is `widget-delete-confirm` (no
        // `dashboard-` prefix) — kept verbatim.
        let mut confirm = Button::new(t("common.delete"))
            .variant(ButtonVariant::Danger)
            .control_size(ControlSize::Large);
        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            let delete_widget = self.on_delete_widget.clone();
            confirm = confirm.on_press(move |s: &mut S| {
                if let Some(id) = state_fn(s).begin_delete_widget() {
                    if let Some(delete_widget) = &delete_widget {
                        delete_widget(s, id);
                    }
                }
            });
        }
        dialog = dialog.footer(confirm.build(k).key("widget-delete-confirm"));

        if let Some(state_fn) = &self.state_fn {
            let state_fn = Rc::clone(state_fn);
            dialog = dialog.on_close(move |s: &mut S| state_fn(s).cancel_delete_widget());
        }
        dialog.build(k, layer);
    }
}
