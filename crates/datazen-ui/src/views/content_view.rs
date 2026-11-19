//! Port of `packages/frontend/src/windows/connection/ContentView.tsx` (view
//! composition only), consumed by `views::connection_page::connection_content`.
//!
//! React's 782-line component is mostly hooks: schema loading, panel/table
//! stores, AI draft bridging, KV slots and a dozen dialog open flags. Those
//! have no Rune equivalents yet; this module ports the *rendered structure*
//! honestly and leaves the missing host capabilities documented rather than
//! faking them:
//!
//! - toolbar: React skips the 48pt band for a KV panel — [`ContentViewVm`]
//!   pins that gate; the host keeps its existing always-mounted key.
//! - tab strip: reused from `views::tab_strip` (Workspace only).
//! - main row: the existing `views::page_body` (query editor / result grid /
//!   table view / home) plus a right-hand drawer host.
//! - split panes: React's `focusedPaneId` keeps a secondary pane per tab. Rune
//!   has no pane store, so the secondary pane is VM-gated and empty.
//! - status bar + dialogs: status bar reused; dialogs are a keyed empty host
//!   because `ContentViewDialogs` and `ContentViewDrawers` have no Rune port.
//!
//! Keys: `content-view`, `content-view-main`, `content-view-panes`,
//! `content-view-split`, `content-view-pane-secondary`, `content-view-drawers`,
//! `content-view-detail`, `content-view-detail-close`, `content-view-ai`,
//! `content-view-ai-close`, `content-view-dialogs`. Existing pinned keys
//! (`content-toolbar`, `tab-strip`/`tab-strip-absent`, `content-status-bar`,
//! `page_body`'s keys) are composed unchanged.
//!
//! Every reusable item carries `#[allow(dead_code)]` with the same removal
//! stage: the panel-store / drawer / dialog view track that wires real content.

use std::rc::Rc;

use rune::tok::{self, type_};
use rune::{col, row, text, El};

use crate::i18n::t;

/// The conditional structure of React `ContentView` that Rune can represent.
#[allow(dead_code)] // VM surface; consumed by connection_page and the tests
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContentViewVm {
    /// A panel tab is active (React `activePanel != null`).
    pub(crate) has_active_panel: bool,
    /// Key-value panel: React hides the whole toolbar band.
    pub(crate) is_kv_panel: bool,
    /// React's `detailOpen` drawer.
    pub(crate) detail_open: bool,
    /// React's `aiChatOpen` drawer.
    pub(crate) ai_chat_open: bool,
    /// React's `detailPanelApplicable` gate for the detail affordance.
    pub(crate) detail_panel_applicable: bool,
    /// The active panel has a second pane (React `focusedPaneId` split).
    pub(crate) split_secondary: bool,
    /// React `activePanel && !isKvPanel` — the toolbar gate.
    pub(crate) react_toolbar_gate: bool,
    /// React `activePanel` — the status-bar gate.
    pub(crate) react_status_gate: bool,
    /// The seven dialogs of React `ContentViewDialogs`; `None` until a host
    /// view owns their open flags (leftover: connection view track).
    pub(crate) dialogs_vm: Option<super::content_view_dialogs::DialogsVm>,
}

impl ContentViewVm {
    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub(crate) fn show_toolbar(&self) -> bool {
        self.has_active_panel && !self.is_kv_panel
    }

    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub(crate) fn show_status_bar(&self) -> bool {
        self.has_active_panel
    }

    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub(crate) fn show_drawers(&self) -> bool {
        (self.detail_open && self.detail_panel_applicable) || self.ai_chat_open
    }

    /// The host currently keeps the toolbar/status keys mounted in every
    /// section (pinned by `tests/connection_page.rs`). React's gates are kept
    /// in the VM for a view that ports the panel store.
    #[allow(dead_code)] // VM surface; consumed by connection_page
    pub(crate) fn from_host(state: &crate::state_def::AppState) -> Self {
        let has_active_panel = state.tabs.selected().is_some();
        Self {
            has_active_panel,
            is_kv_panel: false,
            detail_open: false,
            ai_chat_open: false,
            detail_panel_applicable: has_active_panel,
            split_secondary: false,
            // Host mode keeps both keys mounted in every section, exactly as
            // the pre-port connection_content did (tests/connection_page.rs).
            react_toolbar_gate: false,
            react_status_gate: false,
            dialogs_vm: None,
        }
    }
}

/// Compose the content view from the existing Rune views. `vm` decides the
/// conditional structure; `state`/`theme` feed the reused body functions.
#[allow(dead_code)] // reusable component; mounted by connection_page
pub(crate) fn build(
    state: &crate::state_def::AppState,
    theme: tok::Theme,
    dark: bool,
    vm: &ContentViewVm,
) -> El {
    let mut root = col()
        .key("content-view")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0);

    // 48pt toolbar: React omits it entirely on a KV panel.
    if vm.react_toolbar_gate && vm.show_toolbar() {
        root = root.child(super::content_toolbar(state, theme));
    } else if vm.react_toolbar_gate {
        root = root.child(row().key("content-toolbar-absent").h(0.0));
    } else {
        root = root.child(super::content_toolbar(state, theme));
    }

    root = root.child(if state.active == crate::types::Section::Workspace {
        super::tab_strip(state, theme)
    } else {
        row().key("tab-strip-absent").h(0.0)
    });

    let mut main = row()
        .key("content-view-main")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0);

    let mut panes = col()
        .key("content-view-panes")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(super::page_body(state, theme, dark));

    if vm.split_secondary {
        panes = panes
            .child(row().key("content-view-split").w_full().h(1.0).fill(theme.text_secondary))
            .child(
                row()
                    .key("content-view-pane-secondary")
                    .w_full()
                    .grow(1.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .label("Secondary pane"),
            );
    }
    main = main.child(panes);

    if vm.show_drawers() {
        let mut drawers = col()
            .key("content-view-drawers")
            .w(280.0)
            .min_w(0.0)
            .h_full()
            .gap(8.0);
        if vm.detail_open && vm.detail_panel_applicable {
            drawers = drawers.child(drawer(
                "content-view-detail",
                "content-view-detail-close",
                t("detail.title"),
                theme,
            ));
        }
        if vm.ai_chat_open {
            drawers = drawers.child(drawer("content-view-ai", "content-view-ai-close", "AI", theme));
        }
        main = main.child(drawers);
    } else {
        main = main.child(row().key("content-view-drawers-absent").size(0.0, 0.0));
    }
    root = root.child(main);

    if vm.react_status_gate && vm.show_status_bar() {
        root = root.child(super::content_status_bar(state, theme));
    } else if vm.react_status_gate {
        root = root.child(row().key("content-status-bar-absent").h(0.0));
    } else {
        root = root.child(super::content_status_bar(state, theme));
    }

    // React's seven dialogs. The host has no state slice for their open flags
    // yet (leftover: connection view track), so it supplies an empty VM and a
    // no-op callback; a view that owns the flags can pass both without changing
    // any key here.
    let mut dialog_layer = Vec::new();
    if let Some(dialogs) = vm.dialogs_vm.as_ref() {
        let no_op: Rc<dyn Fn(&mut crate::state_def::AppState, super::content_view_dialogs::DialogAction)> =
            Rc::new(|_state: &mut crate::state_def::AppState, _action| {});
        super::content_view_dialogs::build::<crate::state_def::AppState>(
            dialogs,
            dark,
            &mut dialog_layer,
            &no_op,
        );
    }
    root.child(
        row()
            .key("content-view-dialogs")
            .children(dialog_layer),
    )
}

fn drawer(key: &str, close_key: &str, title: &str, theme: tok::Theme) -> El {
    col()
        .key(key)
        .label(title)
        .w_full()
        .h(160.0)
        .radius(8.0)
        .fill(theme.control_off)
        .px(12.0)
        .py(8.0)
        .gap(8.0)
        .child(
            row()
                .w_full()
                .center_y()
                .child(text(title).font(type_::LABEL).color(theme.text)),
        )
        .child(
            row()
                .key(close_key)
                .role("button")
                .label("Close")
                .focusable(true)
                .w(20.0)
                .h(20.0)
                .center()
                .child(text("×").font(type_::LABEL).color(theme.text_secondary)),
        )
}
