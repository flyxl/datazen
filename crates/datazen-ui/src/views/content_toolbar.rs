//! The 48pt content toolbar — 1:1 port of React `ContentToolbar.tsx`.
//!
//! The React component is controlled entirely by props; this module keeps that
//! seam with [`ContentToolbarVm`] and host callbacks instead of reaching into
//! `AppState`. `views::chrome::content_toolbar` builds the VM from the current
//! section and wires the actions the Rune host already has; a view track can
//! enable the remaining flags without touching this component.
//!
//! React `data-testid` values are kept as element keys:
//! `conn-toolbar-new-query`, `content-toolbar-new-table`,
//! `content-toolbar-er-diagram`, `content-toolbar-objects`,
//! `content-toolbar-privileges`, `conn-toolbar-export`, `conn-toolbar-ai`,
//! `conn-toolbar-kv-context-bar`. The pre-existing `toolbar-new-query` key is
//! kept as a zero-size marker so older probes still find the new-query action.
//!
//! Driver-contributed KV slots and the four lucide glyphs missing from the
//! icon registry (GitFork / Code2 / KeyRound / BookOpen / MessageSquare) cannot
//! be reproduced here: the slot is represented by its keyed container only, and
//! the closest registered glyph is used. No inline icon or colour literal is
//! added. `compact` is a VM field (React measures with a ResizeObserver).

use std::rc::Rc;

use rune::tok::{self, type_};
use rune::{col, icon, row, text, El};

use crate::i18n::t;
use crate::icons::{
    ICON_CONNECTIONS, ICON_DOWNLOAD, ICON_PANEL_LEFT_CLOSE, ICON_PANEL_LEFT_OPEN, ICON_PLUS,
    ICON_SETTINGS, ICON_SPARKLES, ICON_TABLE, ICON_TERMINAL, ICON_WORKFLOWS,
};

/// React `ToolbarShell` band height (`h-12 min-h-[48px]`).
pub const TOOLBAR_HEIGHT: f32 = 48.0;

/// React `useCompactToolbar` constants.
pub const TOOLBAR_EXPANDED_BUTTON_WIDTH: f32 = 96.0;
pub const TOOLBAR_GAP: f32 = 8.0;
pub const TOOLBAR_HORIZONTAL_PADDING: f32 = 32.0;

/// React `estimateExpandedToolbarWidth`.
#[allow(dead_code)] // contract helper; consumed by the audit tests and the host view
pub fn estimate_expanded_toolbar_width(
    expanded_button_count: usize,
    fixed_extra_width: f32,
) -> f32 {
    let count = expanded_button_count;
    TOOLBAR_HORIZONTAL_PADDING
        + count as f32 * TOOLBAR_EXPANDED_BUTTON_WIDTH
        + count.saturating_sub(1) as f32 * TOOLBAR_GAP
        + fixed_extra_width
}

/// React `contentToolbarExpandedMinWidth` — the exact left-button arithmetic.
#[allow(dead_code)] // contract helper; consumed by the audit tests and the host view
pub fn content_toolbar_expanded_min_width(vm: &ContentToolbarVm) -> f32 {
    let mut left_buttons = 0usize;
    if vm.show_new_query {
        left_buttons += 1;
    }
    if vm.show_new_table {
        left_buttons += 1;
    }
    if vm.show_er_diagram {
        left_buttons += 1;
    }
    if vm.show_objects {
        left_buttons += 2;
    }
    if vm.show_batch_export && vm.show_new_query {
        left_buttons += 1;
    }
    let right_cluster = 120.0;
    let detail_toggle = if vm.detail_panel_applicable { 36.0 } else { 0.0 };
    estimate_expanded_toolbar_width(
        left_buttons,
        TOOLBAR_GAP + right_cluster + detail_toggle,
    )
}

/// The state relay of an active KV panel. Only the selected key matters: an
/// absent or blank key means the AI button must not be offered.
#[allow(dead_code)] // VM surface; wired by the KV view track
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KvPanelState {
    pub selected_key: Option<String>,
}

impl KvPanelState {
    /// React `hasKvAiFacts({ selectedKey })` — a blank key is no fact.
    #[allow(dead_code)] // VM surface; consumed by show_ai_chat and the tests
    pub fn has_ai_facts(&self) -> bool {
        self.selected_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
    }
}

/// React `ContentToolbarProps` as a cloneable value.
#[allow(dead_code)] // VM surface; built by chrome::content_toolbar and the tests
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentToolbarVm {
    pub show_new_query: bool,
    pub show_new_table: bool,
    pub show_er_diagram: bool,
    pub show_objects: bool,
    pub show_batch_export: bool,
    pub ai_chat_open: bool,
    pub detail_panel_applicable: bool,
    pub detail_open: bool,
    /// A driver contributed a context bar (`contextBarSlot` present).
    pub has_context_bar: bool,
    /// `undefined` on a relational panel; present on a KV panel.
    pub kv_panel: Option<KvPanelState>,
    /// `useCompactToolbar` hides labels in narrow bands.
    pub compact: bool,
    /// Resolved `t("common.newQuery")` strings. The T0-03 sweep pins two calls
    /// to that key in `views/chrome.rs`, so the host supplies both the visible
    /// label and the accessible label instead of the builder calling `t`.
    pub new_query_label: String,
    pub new_query_aria: String,
    /// Section title shown at the left edge (existing `content-toolbar-title`).
    pub title: String,
    /// The docs and AI buttons are React's always-on right cluster. The Rune
    /// host has no docs-window / AI-chat adapter yet, so it leaves these false
    /// to keep the existing focus order; the controlled component still
    /// renders them for a view that wires the actions.
    pub show_docs: bool,
    pub show_ai: bool,
}

impl ContentToolbarVm {
    /// React `showAiChat`: no relay (relational) or a KV panel with a key.
    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub fn show_ai_chat(&self) -> bool {
        self.kv_panel
            .as_ref()
            .map_or(true, KvPanelState::has_ai_facts)
    }
}

type Action<S> = Rc<dyn Fn(&mut S)>;

#[allow(dead_code)] // callback bundle; wired by chrome::content_toolbar
struct Actions<S> {
    new_query: Option<Action<S>>,
    create_table: Option<Action<S>>,
    er_diagram: Option<Action<S>>,
    objects: Option<Action<S>>,
    privileges: Option<Action<S>>,
    batch_export: Option<Action<S>>,
    docs: Option<Action<S>>,
    ai_chat: Option<Action<S>>,
    detail: Option<Action<S>>,
}

impl<S> Default for Actions<S> {
    fn default() -> Self {
        Self {
            new_query: None,
            create_table: None,
            er_diagram: None,
            objects: None,
            privileges: None,
            batch_export: None,
            docs: None,
            ai_chat: None,
            detail: None,
        }
    }
}

/// Builder for the ported toolbar.
#[allow(dead_code)] // reusable component; mounted by chrome::content_toolbar
pub struct ContentToolbar<'a, S: 'static> {
    vm: &'a ContentToolbarVm,
    actions: Actions<S>,
}

#[allow(dead_code)] // reusable component; mounted by chrome::content_toolbar
impl<'a, S: 'static> ContentToolbar<'a, S> {
    pub fn new(vm: &'a ContentToolbarVm) -> Self {
        Self {
            vm,
            actions: Actions::default(),
        }
    }

    pub fn on_new_query(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.new_query = Some(Rc::new(f));
        self
    }
    pub fn on_create_table(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.create_table = Some(Rc::new(f));
        self
    }
    pub fn on_er_diagram(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.er_diagram = Some(Rc::new(f));
        self
    }
    pub fn on_objects(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.objects = Some(Rc::new(f));
        self
    }
    pub fn on_privileges(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.privileges = Some(Rc::new(f));
        self
    }
    pub fn on_batch_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.batch_export = Some(Rc::new(f));
        self
    }
    pub fn on_docs(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.docs = Some(Rc::new(f));
        self
    }
    pub fn on_ai_chat(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.ai_chat = Some(Rc::new(f));
        self
    }
    pub fn on_detail(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.detail = Some(Rc::new(f));
        self
    }

    pub fn build(self, theme: tok::Theme) -> El {
        let vm = self.vm;
        let mut bar = row()
            .key("content-toolbar")
            .role("toolbar")
            .w_full()
            .h(TOOLBAR_HEIGHT)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .fill(theme.bg);

        if !vm.title.is_empty() {
            bar = bar.child(
                text(&vm.title)
                    .key("content-toolbar-title")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        }

        if vm.has_context_bar {
            bar = bar.child(
                row()
                    .key("conn-toolbar-kv-context-bar")
                    .label("kv-context-bar")
                    .grow(1.0)
                    .min_w(0.0)
                    .h(TOOLBAR_HEIGHT),
            );
        }

        if vm.show_new_query {
            bar = bar.child(button(
                &theme,
                "conn-toolbar-new-query",
                Some("toolbar-new-query"),
                &vm.new_query_label,
                &vm.new_query_aria,
                &ICON_PLUS,
                vm.compact,
                false,
                &self.actions.new_query,
            ));
        }
        if vm.show_new_table {
            bar = bar.child(button(
                &theme,
                "content-toolbar-new-table",
                None,
                t("common.newTable"),
                t("common.newTable"),
                &ICON_TABLE,
                vm.compact,
                false,
                &self.actions.create_table,
            ));
        }
        if vm.show_er_diagram {
            // React tags both the wrapper span and the inner button; the e2e
            // helper queries the inner key.
            let inner = button(
                &theme,
                "content-toolbar-er-diagram-button",
                None,
                t("common.erDiagram"),
                t("common.erDiagram"),
                &ICON_WORKFLOWS,
                vm.compact,
                false,
                &self.actions.er_diagram,
            );
            bar = bar.child(row().key("content-toolbar-er-diagram").child(inner));
        }
        if vm.show_objects {
            bar = bar.child(button(
                &theme,
                "content-toolbar-objects",
                None,
                t("objects.title"),
                t("objects.title"),
                &ICON_CONNECTIONS,
                vm.compact,
                false,
                &self.actions.objects,
            ));
            bar = bar.child(button(
                &theme,
                "content-toolbar-privileges",
                None,
                t("privileges.title"),
                t("privileges.title"),
                &ICON_SETTINGS,
                vm.compact,
                false,
                &self.actions.privileges,
            ));
        }
        if vm.show_batch_export && vm.show_new_query {
            bar = bar.child(button(
                &theme,
                "conn-toolbar-export",
                None,
                t("batchExport.title"),
                t("batchExport.title"),
                &ICON_DOWNLOAD,
                vm.compact,
                false,
                &self.actions.batch_export,
            ));
        }

        if !vm.has_context_bar {
            bar = bar.child(row().grow(1.0).min_w(0.0));
        }

        if vm.show_docs {
            bar = bar.child(button(
                &theme,
                "content-toolbar-docs",
                None,
                t("docs.openAiHelp"),
                t("docs.openAiHelp"),
                &ICON_TERMINAL,
                true,
                false,
                &self.actions.docs,
            ));
        }

        if vm.show_ai && vm.show_ai_chat() {
            // React keeps the visible "AI" label but sets `title` to the KV
            // context tooltip whenever a KV panel relay is present.
            let ai_title = if vm.kv_panel.is_some() {
                t("redis.ai.context.tooltip")
            } else {
                "AI"
            };
            bar = bar.child(button(
                &theme,
                "conn-toolbar-ai",
                None,
                "AI",
                ai_title,
                &ICON_SPARKLES,
                true,
                vm.ai_chat_open,
                &self.actions.ai_chat,
            ));
        }

        if vm.detail_panel_applicable {
            // DetailPanelToggle swaps its title with the open state.
            let detail_title = if vm.detail_open {
                t("detail.hide")
            } else {
                t("detail.show")
            };
            bar = bar.child(button(
                &theme,
                "content-toolbar-detail-toggle",
                None,
                detail_title,
                detail_title,
                if vm.detail_open {
                    &ICON_PANEL_LEFT_CLOSE
                } else {
                    &ICON_PANEL_LEFT_OPEN
                },
                true,
                vm.detail_open,
                &self.actions.detail,
            ));
        }
        bar
    }
}

fn button<S: 'static>(
    theme: &tok::Theme,
    key: &str,
    legacy_key: Option<&str>,
    label: &str,
    aria: &str,
    glyph: &'static rune::Icon,
    compact: bool,
    active: bool,
    action: &Option<Action<S>>,
) -> El {
    let mut el = row()
        .key(key)
        .role("button")
        .label(aria)
        .focusable(true)
        .h(32.0)
        .px(10.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .fill(if active {
            theme.control_off
        } else {
            rune::Color::TRANSPARENT
        })
        .child(icon(glyph, 14.0).color(theme.text_secondary).a11y_hidden());
    if !compact {
        el = el.child(text(label).font(type_::LABEL).color(theme.text));
    }
    if let Some(legacy) = legacy_key {
        el = el.child(
            col()
                .key(legacy)
                .size(0.0, 0.0)
                .absolute(0.0, 0.0)
                .a11y_hidden(),
        );
    }
    if let Some(action) = action.clone() {
        el = el.on_press(move |s: &mut S| action(s));
    }
    el
}
