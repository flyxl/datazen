//! `McpPromoBar`, ported 1:1 from
//! `packages/frontend/src/windows/connection/home/McpPromoBar.tsx`.
//!
//! React renders a single row: a sparkles icon in an accent tint, the
//! `DataZen MCP Server` title, a one-line localized description, the CLI
//! command in a terminal-prefixed mono box and a Copy button. The copy
//! affordance owns a 2000 ms "Copied" feedback window: the real clipboard is
//! written through `Tx`, a real runtime wake is scheduled with `wake_after`
//! and the deadline is recorded in host state. There is no never-expiring
//! boolean and no fake async (no threads, no simulated IPC).
//!
//! # Consumer / removal stage
//!
//! No production host wiring yet — `views/connections_home.rs` still renders
//! its own inline promo bar for the A1 landing. The home-wiring task will
//! replace that block with [`build_mcp_promo_bar`] and feed it the host's
//! executable path; until then every reusable item carries a per-item
//! `#[allow(dead_code)]` with this removal stage.
//!
//! # Keys
//!
//! `home-mcp-promo-bar` (root), `home-mcp-copy-button` (copy trigger); the
//! command box and title carry a11y labels where React used `title` text.
//! All copy strings use exact locale-pack keys (`connWin.home.mcp.shortDesc`,
//! `connWin.home.aiIntegration.copy`, `connWin.home.aiIntegration.copied`).

use std::time::Duration;

use rune::{col, icon, row, text, tok, El, TextStyle, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CHECK, ICON_COPY, ICON_SPARKLES, ICON_TERMINAL};
use crate::state_def::AppState;
use crate::ui::kit::kit;
use crate::ui::palette;

/// React `COPIED_FEEDBACK_MS`.
#[allow(dead_code)] // feedback constant; consumed by the home wiring and tests.
pub const MCP_COPIED_FEEDBACK_MS: u64 = 2000;

/// Synthetic row id for the single bar-level copy marker. The A1 home uses the
/// same marker so the shared `home_copy_tick` reverts it without extra state.
#[allow(dead_code)] // marker id; consumed by the action handler and tests.
pub const MCP_COPY_ROW_ID: &str = "__mcp__";

/// React `formatMcpCliCommand` default when no executable path is available.
#[allow(dead_code)] // default command; consumed by the VM and tests.
pub const MCP_DEFAULT_COMMAND: &str = "datazen --mcp";

/// Host-owned snapshot of `McpPromoBar` props plus its feedback deadline. The
/// deadline lives in [`AppState`] (`home_copied_until` / `home_copied_id`) so
/// the app poll can revert the label when the real wake fires at expiry.
#[allow(dead_code)] // VM surface; consumed by the home wiring and tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct McpPromoVm {
    /// The CLI command shown and copied (React `mcpCliCommand`).
    pub cli_command: String,
}

#[allow(dead_code)] // VM constructor; consumed by the home wiring and tests.
impl McpPromoVm {
    /// A VM using React's default launch command.
    pub fn with_default_command() -> Self {
        Self {
            cli_command: MCP_DEFAULT_COMMAND.to_owned(),
        }
    }
}

/// View-emitted event for the host. No SQL/IPC/threads are faked here; the
/// host applies it through [`apply_mcp_promo_action`], which performs the real
/// clipboard write and arms the real wake.
#[allow(dead_code)] // host port; remove when the shared copy-feedback host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum McpPromoAction {
    Copy(String),
}

/// Apply a promo-bar event: write the exact text to the clipboard, schedule a
/// real 2000 ms wake and record the matching deadline in host state. The app
/// poll's `home_copy_tick` clears the marker once the runtime clock reaches it.
#[allow(dead_code)] // host port; remove when the shared copy-feedback host lands.
pub fn apply_mcp_promo_action(
    state: &mut AppState,
    action: McpPromoAction,
    tx: &mut Tx,
) {
    match action {
        McpPromoAction::Copy(command) => {
            tx.copy(command);
            tx.wake_after(Duration::from_millis(MCP_COPIED_FEEDBACK_MS));
            state.home_copied_until =
                Some(tx.clock() + MCP_COPIED_FEEDBACK_MS as f64 / 1000.0);
            state.home_copied_id = Some(MCP_COPY_ROW_ID.to_owned());
        }
    }
}

/// Whether the bar-level "Copied" marker is currently active.
#[allow(dead_code)] // rendering helper; delete with the host wiring.
pub fn mcp_copied(state: &AppState) -> bool {
    state.home_copied_until.is_some()
        && state.home_copied_id.as_deref() == Some(MCP_COPY_ROW_ID)
}

/// Build the promo bar. The copy button emits [`McpPromoAction::Copy`] through
/// [`apply_mcp_promo_action`] so the feedback is a real timed wake, not a flag.
#[allow(dead_code)] // consumer: the connections-home host that mounts this view (not landed); delete when it replaces the inline promo bar.
pub fn build_mcp_promo_bar(
    vm: &McpPromoVm,
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
) -> El {
    let accent = crate::ui::theme::accent(dark);
    let command = vm.cli_command.clone();
    row()
        .key("home-mcp-promo-bar")
        .w_full()
        .gap(12.0)
        .center_y()
        .radius(12.0)
        .fill(theme.card)
        .border(1.0, theme.card_border)
        .px(14.0)
        .py(10.0)
        .child(
            row()
                .size(32.0, 32.0)
                .radius(8.0)
                .center()
                .fill(palette::accent_tint_fill(dark))
                .child(icon(&ICON_SPARKLES, 16.0).color(accent)),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(
                    text("DataZen MCP Server")
                        .font(TextStyle::new(13.0, 600.0))
                        .color(theme.text),
                )
                .child(
                    text(t("connWin.home.mcp.shortDesc"))
                        .font(tok::type_::LABEL)
                        .color(theme.text_secondary)
                        .wrap()
                        .max_lines(2),
                ),
        )
        .child(
            row()
                .label(command.clone())
                .h(30.0)
                .max_w(360.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .radius(8.0)
                .fill(theme.bg)
                .border(1.0, theme.card_border)
                .child(icon(&ICON_TERMINAL, 13.0).color(theme.text_secondary))
                .child(text(command.clone()).font(tok::type_::MONO_CAPTION).color(theme.text)),
        )
        .child(copy_button(command, state, dark))
}

/// The ghost copy button flips icon + label while the real feedback deadline
/// is live, then reverts when the host poll clears the marker.
#[allow(dead_code)] // consumer: build_mcp_promo_bar, itself awaiting the connections-home host mount; delete with that wiring.
fn copy_button(command: String, state: &AppState, dark: bool) -> El {
    let copied = mcp_copied(state);
    let label = if copied {
        t("connWin.home.aiIntegration.copied")
    } else {
        t("connWin.home.aiIntegration.copy")
    };
    let glyph = if copied { &ICON_CHECK } else { &ICON_COPY };
    let mut button = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(glyph);
    button = button.on_press_tx(move |state: &mut AppState, tx: &mut Tx| {
        apply_mcp_promo_action(state, McpPromoAction::Copy(command.clone()), tx);
    });
    button.build(kit(dark)).key("home-mcp-copy-button")
}
