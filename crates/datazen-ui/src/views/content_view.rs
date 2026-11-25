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
use crate::state_def::AppState;

/// The conditional structure of React `ContentView` that Rune can represent.
#[allow(dead_code)] // VM surface; consumed by connection_page and the tests
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentViewVm {
    /// A panel tab is active (React `activePanel != null`).
    pub has_active_panel: bool,
    /// Key-value panel: React hides the whole toolbar band.
    pub is_kv_panel: bool,
    /// React's `detailOpen` drawer.
    pub detail_open: bool,
    /// React's `aiChatOpen` drawer.
    pub ai_chat_open: bool,
    /// React's `detailPanelApplicable` gate for the detail affordance.
    pub detail_panel_applicable: bool,
    /// The active panel has a second pane (React `focusedPaneId` split).
    pub split_secondary: bool,
    /// React `activePanel && !isKvPanel` — the toolbar gate.
    pub react_toolbar_gate: bool,
    /// React `activePanel` — the status-bar gate.
    pub react_status_gate: bool,
    /// The seven dialogs of React `ContentViewDialogs`; `None` until a host
    /// view owns their open flags (leftover: connection view track).
    pub dialogs_vm: Option<super::content_view_dialogs::DialogsVm>,
    /// The selected database object; `None` until a host object store owns one.
    pub object_vm: Option<super::database_object_view::DatabaseObjectVm>,
    /// The document browser; `None` until a host owns a document connection.
    pub document_vm: Option<super::document_connection_view::DocumentViewVm>,
}

impl ContentViewVm {
    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub fn show_toolbar(&self) -> bool {
        self.has_active_panel && !self.is_kv_panel
    }

    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub fn show_status_bar(&self) -> bool {
        self.has_active_panel
    }

    #[allow(dead_code)] // VM surface; consumed by build and the tests
    pub fn show_drawers(&self) -> bool {
        (self.detail_open && self.detail_panel_applicable) || self.ai_chat_open
    }

    /// The host currently keeps the toolbar/status keys mounted in every
    /// section (pinned by `tests/connection_page.rs`). React's gates are kept
    /// in the VM for a view that ports the panel store.
    #[allow(dead_code)] // VM surface; consumed by connection_page
    pub fn from_host(state: &crate::state_def::AppState) -> Self {
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
            object_vm: None,
            document_vm: None,
        }
    }
}

/// Compose the content view from the existing Rune views. `vm` decides the
/// conditional structure; `state`/`theme` feed the reused body functions.
#[allow(dead_code)] // reusable component; mounted by connection_page
pub fn build(
    state: &crate::state_def::AppState,
    theme: tok::Theme,
    dark: bool,
    vm: &ContentViewVm,
    layer: &mut Vec<El>,
) -> El {
    let mut root = col()
        .key("content-view")
        .w_full()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0);

    // 48pt toolbar: React omits it entirely on a KV panel.
    if state.native_kv.is_some() && state.active == crate::types::Section::Workspace {
        root = root.child(row().key("content-toolbar-absent").h(0.0));
    } else if vm.react_toolbar_gate && vm.show_toolbar() {
        root = root.child(super::content_toolbar(state, theme));
    } else if vm.react_toolbar_gate {
        root = root.child(row().key("content-toolbar-absent").h(0.0));
    } else {
        root = root.child(super::content_toolbar(state, theme));
    }

    root = root.child(if state.active == crate::types::Section::Workspace && state.native_kv.is_none() {
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
        // The single window overlay layer is owned by `connection_page` and
        // threaded here: `page_body` appends the workspace's select listboxes
        // to it, and the page mounts the layer exactly once at its root.
        .child(super::page_body(state, theme, dark, layer));

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
        // T245: the drawer host is the real `ContentViewDrawers` port. The VM
        // carries the two flags; every other input defaults (the host that owns
        // the panel store will fill it). The pinned drawer keys are preserved by
        // the port itself.
        let drawer_vm = super::content_view_drawers::DrawerState {
            detail_open: vm.detail_open,
            ai_chat_open: vm.ai_chat_open,
            detail_panel_applicable: vm.detail_panel_applicable,
            // The pre-port host has no db-session id, but its existing tests
            // pin that `ai_chat_open` alone mounts the AI drawer. Supply the
            // flag as the gate so the old composition behavior is preserved.
            has_db_session: vm.ai_chat_open,
            panel_id: String::new(),
            panel_kind: super::content_view_drawers::PanelKind::Other,
            focused_pane: None,
            panes: std::collections::HashMap::new(),
            editor_selection: String::new(),
            verdict: None,
            resolving_origin: false,
            key_props_slot: false,
            // The pre-port host has no resizable sidebar; use the minimum so
            // the AI drawer fits the fixed 280pt drawer column at 960px.
            sidebar_width: 240.0,
            table_columns: Vec::new(),
            table_rows: Vec::new(),
            table_detail_row_index: None,
            kv_selected_key: None,
        };
        let mut layer = Vec::new();
        super::content_view_drawers::build::<AppState>(
            &drawer_vm,
            None::<std::rc::Rc<dyn Fn(&mut AppState, super::content_view_drawers::DrawerAction) + 'static>>,
            dark,
            &mut layer,
        );
        // The port mounts the `content-view-drawers` root and its drawers.
        main = main.children(layer);
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
    // Dialog scrim/layer elements are `absolute(0,0).w_full().h_full()`, so the
    // host slot must itself be a full-window overlay or they anchor to the page
    // bottom (y=640 in the minimum window) and render off-screen. Keep the
    // empty slot zero-sized so the host layout is untouched when no dialog is
    // mounted.
    let slot = row().key("content-view-dialogs");
    let slot = if dialog_layer.is_empty() {
        slot.size(0.0, 0.0)
    } else {
        slot.absolute(0.0, 0.0).w_full().h_full()
    };
    root.child(slot.children(dialog_layer))
}
