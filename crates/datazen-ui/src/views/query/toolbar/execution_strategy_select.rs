//! `ExecutionStrategySelect`, ported 1:1 from
//! `packages/frontend/src/windows/connection/query/toolbar/ExecutionStrategySelect.tsx`.
//!
//! React is an icon-only toolbar button showing the active strategy in its
//! tooltip/aria label; pressing it opens a native context menu of four items
//! whose labels carry a `✓` on the current choice. Picking one persists it via
//! `updateSettings`. The Rune port keeps the same observable contract with an
//! injected host port: [`ExecutionStrategyAction::Select`] carries the wire id
//! the host writes to settings. No command, promise or thread runs here.
//!
//! ## Documented deviations
//!
//! * Rune has no native context-menu port, so the menu is an inline keyed
//!   dropdown. Remove it when `showNativeContextMenu` lands.
//! * React's trigger composes two lucide glyphs (`ListFilter` + `ChevronDown`);
//!   the registry's `ICON_FILTER` stands in for the pair (the second glyph is
//!   pure decoration on an icon-only button).
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this button yet; the query toolbar will own the
//! VM and map `Select` onto its settings write. Until then every reusable item
//! carries a per-item `#[allow(dead_code)]` with this removal stage.

use std::rc::Rc;

use rune::{col, text, El};

use crate::views::query::query_editor_section::ExecutionStrategy;
use crate::i18n::t;
use crate::icons::ICON_FILTER;
use crate::ui::kit::kit;
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant};

const TITLE_KEY: &str = "query.executionStrategy.title";
const TRIGGER_KEY: &str = "editor-execution-strategy-button";

/// React `SqlExecutionStrategy` wire ids. Kept local so the shared enum in
/// `query_editor_section` stays the single type; this is only its wire spelling.
#[allow(dead_code)] // pure mapping; consumed by the menu items and tests.
pub fn strategy_id(strategy: ExecutionStrategy) -> &'static str {
    match strategy {
        ExecutionStrategy::CurrentStatement => "current_statement",
        ExecutionStrategy::EntireScript => "entire_script",
        ExecutionStrategy::LargestStatement => "largest_statement",
        ExecutionStrategy::Ask => "ask",
    }
}

/// React `STRATEGIES` order, as the shared enum's declared order.
#[allow(dead_code)] // pure projection; consumed by the menu and tests.
pub fn all_strategies() -> [ExecutionStrategy; 4] {
    ExecutionStrategy::ALL
}

/// Host-owned projection of the component props plus `settings.sqlExecutionStrategy`.
#[allow(dead_code)] // VM surface; owned by the query toolbar host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionStrategySelectVm {
    /// React defaults to `current_statement` when the setting is unset.
    pub strategy: ExecutionStrategy,
    /// Rune needs the popup state from the caller (React's native menu owns it).
    pub open: bool,
    pub icon_only: bool,
    pub disabled: bool,
}

impl ExecutionStrategySelectVm {
    /// React's `activeLabelKey` lookup with the current-statement fallback.
    #[allow(dead_code)] // pure helper; consumed by the trigger and tests.
    pub fn active_label_key(&self) -> &'static str {
        self.strategy.label_key()
    }

    /// React's composed `title`: `<title key>: <active label key>`.
    #[allow(dead_code)] // pure helper; consumed by the trigger and tests.
    pub fn title(&self) -> String {
        format!("{}: {}", t(TITLE_KEY), t(self.active_label_key()))
    }
}

/// The host intents this control can emit.
#[allow(dead_code)] // host port; remove when the toolbar host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionStrategyAction {
    ToggleMenu,
    Select(ExecutionStrategy),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ExecutionStrategyAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: ExecutionStrategyAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the trigger and its optional dropdown.
#[allow(dead_code)] // consumer: the query toolbar host (not landed).
pub fn build<S: 'static>(
    vm: &ExecutionStrategySelectVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let label = t(vm.active_label_key()).to_owned();
    let k = kit(dark);
    let mut trigger = ToolbarButton::new(TRIGGER_KEY, label.clone())
        .icon(&ICON_FILTER)
        .icon_only(vm.icon_only)
        .variant(ToolbarButtonVariant::Ghost)
        .disabled(vm.disabled);
    if let Some(callback) = on_action.clone() {
        trigger = trigger.on_press(move |s: &mut S| callback(s, ExecutionStrategyAction::ToggleMenu));
    }
    let trigger = trigger
        .build(dark)
        .key(TRIGGER_KEY)
        .label(label)
        .help(vm.title());

    let mut root = col()
        .key("execution-strategy-select")
        .gap(4.0)
        .child(trigger);
    if vm.open {
        let mut menu = col()
            .key("execution-strategy-menu")
            .role("menu")
            .label(t(TITLE_KEY))
            .gap(2.0);
        for strategy in all_strategies() {
            let check = if strategy == vm.strategy { "✓ " } else { "   " };
            let item_label = format!("{check}{}", t(strategy.label_key()));
            menu = menu.child(
                text(item_label.clone())
                    .key(format!("strategy-{}", strategy_id(strategy)))
                    .role("menuitem")
                    .focusable(true)
                    .label(item_label)
                    .color(k.c.text_base)
                    .on_press(emit(&on_action, ExecutionStrategyAction::Select(strategy))),
            );
        }
        root = root.child(menu);
    }
    root
}
