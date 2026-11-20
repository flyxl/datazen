//! The AI assistant aside in the connection workspace — T009-S7,
//! the `Section` wiring for T009.
//!
//! React mounts `<AiChatPanel>` in `ContentViewDrawers.tsx` as a
//! right-hand `<aside style={{ width: aiSidebarWidth }}>` next to the
//! content when `aiChatOpen && dbSessionId`, with a 1pt resize handle
//! (`useResizable`: 320 initial, 240–600). The Rune aside is the same
//! shape: [`splitter::splitter_handle_reversed`] plus a fixed-width
//! body holding the S6 shell. The header's selects push their
//! listboxes into the `layer` the root appends after the body (the
//! same window-level layer `modal_layer` uses), so the aside builder
//! takes it as a parameter instead of owning one.
//!
//! # Host-owned props
//!
//! The shell's three host props and two window openers are passed
//! through only where Rune has a real target:
//!
//! * `onInsertSql` — React's `updateQuerySql` *replaces* the active
//!   query editor's SQL; the callback delegates to the host's
//!   [`AppState::insert_ai_sql`], which only fires with a query tab
//!   selected, exactly as React only calls it for a `query` panel.
//! * `onNewQuery` — React opens a new query tab prefilled with the
//!   code block; the callback delegates to
//!   [`AppState::open_ai_new_query`]. Both are UI-unreachable until
//!   T169 lands real assistant content, so no runtime test drives
//!   them — the host methods are unit-tested directly instead.
//! * `on_open_settings` — the unconfigured view's button owns
//!   `openSettingsWindow('ai')` in React; Rune opens its own settings
//!   page at the `ai` section.
//! * `onRunCode`, `on_open_ai_help`, `on_open_workflow_help` stay
//!   unwired: Rune has no run-a-code-block gate and no docs window.
//!   T088/T169 own those seams.
//!
//! # Deliberate divergences
//!
//! * React gates the mount on a truthy `dbSessionId`; Rune has no db
//!   session identity in `AppState`, so the aside mounts whenever
//!   `ai_chat_open` is set. The panel's session key falls back to
//!   `'default'` exactly as React's `computeSessionKey` does when both
//!   ids are empty, and T088 re-adds the gate when it wires the real
//!   session identity.
//! * The aside fill is the page background (`theme.bg`): the panel
//!   root draws no fill of its own, so the body fill *is* the panel
//!   background, and no surface token was invented for it.

use crate::components::ai::AiChatPanel;
use crate::constants::*;
use crate::splitter;
use crate::state_def::AppState;
use crate::types::Section;
use rune::tok;
use rune::{col, row, El};

/// The AI aside: resize handle + fixed-width body holding the S6
/// panel shell. Rendered by `workspace_body` when `ai_chat_open`.
pub(crate) fn ai_chat_aside(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let width =
        splitter::splitter_size(state, AI_SIDEBAR_WIDTH_KEY, AI_SIDEBAR_WIDTH_DEFAULT) as f32;
    let handle = splitter::splitter_handle_reversed(
        state,
        AI_SIDEBAR_WIDTH_KEY,
        splitter::SplitAxis::Vertical,
        AI_SIDEBAR_WIDTH_MIN,
        AI_SIDEBAR_WIDTH_MAX,
        AI_SIDEBAR_WIDTH_DEFAULT,
    );
    let panel = AiChatPanel::new(&state.ai_chat_panel)
        .panel_mut(|state: &mut AppState| &mut state.ai_chat_panel)
        .on_insert_sql(AppState::insert_ai_sql)
        .on_new_query(AppState::open_ai_new_query)
        .on_open_settings(|state: &mut AppState| {
            state.active = Section::Settings;
            state.set_settings_section("ai");
        })
        .build(dark, layer);
    row().key("ai-chat-aside").h_full().child(handle).child(
        col()
            .key("ai-chat-aside-body")
            .w(width)
            .h_full()
            .min_w(0.0)
            .min_h(0.0)
            .fill(theme.bg)
            .child(panel),
    )
}
