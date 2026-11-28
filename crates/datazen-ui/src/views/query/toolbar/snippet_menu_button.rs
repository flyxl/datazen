//! `SnippetMenuButton`, ported 1:1 from
//! `packages/frontend/src/windows/connection/query/toolbar/SnippetMenuButton.tsx`.
//!
//! React is an icon-only ghost toolbar button (`query.snippets` label,
//! `query.snippetsTitle` tooltip). Pressing it opens the app-wide native
//! context menu with one item per built-in plus user snippet: the item label is
//! the description i18n key, its prefix doubles as the shortcut column, and
//! activating it inserts the template into the editor. The Rune port keeps that
//! observable contract with an injected host port: [`SnippetAction::Insert`]
//! carries the item id and its template; the host performs the real insert.
//! No command, promise, editor handle or timer runs here.
//!
//! Native Query mounts the same trigger and menu content in its root layer.
//! The host owns focus, lifetime, and document edits.

use std::rc::Rc;

use rune::{col, row, text, El, Tx};

use crate::i18n::t;
use crate::icons::ICON_CODE_2;
use crate::ui::kit::kit;
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant};

const LABEL_KEY: &str = "query.snippets";
const TITLE_KEY: &str = "query.snippetsTitle";
const TRIGGER_KEY: &str = "editor-snippets-button";

/// React `SqlSnippetItem`, reduced to what the menu button reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetItemVm {
    pub id: String,
    pub prefix: String,
    pub description_key: String,
    pub template: String,
}

/// The seven `BUILTIN_SQL_SNIPPETS` in React order, verbatim ids/prefixes/keys
/// and templates (the trailing `${n}` tabstop is part of the template).
pub fn builtin_snippets() -> Vec<SnippetItemVm> {
    crate::components::sql_editor::snippets::builtin_sql_snippets().iter().map(|item| SnippetItemVm {
        id: item.id.clone(),
        prefix: item.prefix.clone(),
        description_key: item.description_key.clone(),
        template: item.template.clone(),
    }).collect()
}

/// Host-owned projection of the props plus `settings.sqlSnippets` and the popup
/// state the native menu owns in React.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SnippetMenuButtonVm {
    pub open: bool,
    pub icon_only: bool,
    pub disabled: bool,
    /// React `settings.sqlSnippets ?? []`, appended after the built-ins.
    pub user_snippets: Vec<SnippetItemVm>,
}

impl SnippetMenuButtonVm {
    /// React's menu source: `[...BUILTIN_SQL_SNIPPETS, ...(userSnippets ?? [])]`.
        pub fn menu_items(&self) -> Vec<SnippetItemVm> {
        let mut items = builtin_snippets();
        items.extend(self.user_snippets.iter().cloned());
        items
    }
}

/// The host intents this control can emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnippetAction {
    ToggleMenu,
    Insert { id: String, template: String },
}

type ActionFn<S> = Rc<dyn Fn(&mut S, SnippetAction) + 'static>;

type TxActionFn<S> = Rc<dyn Fn(&mut S, &mut Tx, SnippetAction) + 'static>;

pub(crate) fn trigger<S: 'static>(
    vm: &SnippetMenuButtonVm, key: &str, on_action: Option<TxActionFn<S>>, dark: bool,
) -> El {
    let label = t(LABEL_KEY).to_owned();
    let mut trigger = ToolbarButton::<S>::new(key, label.clone())
        .icon(&ICON_CODE_2)
        .icon_only(vm.icon_only)
        .variant(ToolbarButtonVariant::Ghost)
        .disabled(vm.disabled)
        .build(dark).label(label).help(t(TITLE_KEY)).expanded(vm.open);
    if vm.icon_only {
        trigger = trigger.size(28.0, 28.0).shrink();
    }
    if let Some(callback) = on_action.filter(|_| !vm.disabled) {
        trigger = trigger.on_press_tx(move |s: &mut S, tx| callback(s, tx, SnippetAction::ToggleMenu));
    }
    trigger
}

/// Shared ordered menu content. The live adapter supplies literal saved names
/// rather than treating arbitrary user text as translation keys.
pub(crate) fn menu<S: 'static>(
    vm: &SnippetMenuButtonVm, literal_label: impl Fn(&str) -> Option<String>, show_prefix: bool,
    on_action: Option<TxActionFn<S>>, dark: bool,
) -> El {
    let k = kit(dark);
    let mut menu = col().key("snippet-menu").role("menu").label(t(LABEL_KEY)).gap(2.0);
    if !show_prefix {
        menu = menu.focus_group().w_full().min_h(0.0).shrink().px(2.0).py(4.0).scroll(rune::ScrollSpec { stick_bottom: false });
    }
    for item in vm.menu_items() {
        let id = item.id;
        let description = literal_label(&id).unwrap_or_else(|| t(&item.description_key).to_owned());
        let mut label = text(description.clone()).key(format!("snippet-{id}-label"))
            .font(k.ty.text_small).color(k.c.text_base);
        if !show_prefix {
            label = label.min_w(0.0).truncate();
        }
        let mut row_el = row().key(format!("snippet-{id}"))
            .role("menuitem").focusable(true).label(description).gap(8.0)
            .center_y().child(label);
        if show_prefix {
            row_el = row_el.child(text(item.prefix).key(format!("snippet-{id}-shortcut"))
                .font(k.ty.mono).color(k.c.text_muted));
        } else {
            row_el = row_el.w_full().h(32.0).shrink().px(8.0).radius(4.0)
                .hover(k.c.layer_02).focus_highlight(k.c.layer_02);
        }
        if let Some(on_action) = on_action.clone() {
            let template = item.template;
            row_el = row_el.on_press_tx(move |s: &mut S, tx| {
                on_action(s, tx, SnippetAction::Insert { id: id.clone(), template: template.clone() });
            });
        }
        menu = menu.child(row_el);
    }
    menu
}

/// Inline composition kept for embedders that do not own a root popup layer.
pub fn build<S: 'static>(vm: &SnippetMenuButtonVm, on_action: Option<ActionFn<S>>, dark: bool) -> El {
    let tx_action: Option<TxActionFn<S>> = on_action.map(|f| {
        let call: TxActionFn<S> = Rc::new(move |s, _, action| f(s, action));
        call
    });
    let mut root = col().key("snippet-menu-button").gap(4.0)
        .child(trigger(vm, TRIGGER_KEY, tx_action.clone(), dark));
    if vm.open {
        root = root.child(menu(vm, |_| None, true, tx_action, dark));
    }
    root
}
