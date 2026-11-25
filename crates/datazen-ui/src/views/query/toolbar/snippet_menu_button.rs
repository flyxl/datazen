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
//! ## Documented deviations
//!
//! * Rune has no native context-menu port, so the menu is an inline keyed
//!   dropdown (as in `execution_strategy_select`). Remove it when
//!   `showNativeContextMenu` lands.
//! * The registry has no `Code2` glyph; `ICON_TERMINAL` stands in for it.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this button yet; the query toolbar owns the VM,
//! user snippets and menu open state, and maps `Insert` onto the editor's
//! `insertSnippet`. Until then every reusable item carries a per-item
//! `#[allow(dead_code)]` with this removal stage.

use std::rc::Rc;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::icons::ICON_TERMINAL;
use crate::ui::kit::kit;
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant};

const LABEL_KEY: &str = "query.snippets";
const TITLE_KEY: &str = "query.snippetsTitle";
const TRIGGER_KEY: &str = "editor-snippets-button";

/// React `SqlSnippetItem`, reduced to what the menu button reads.
#[allow(dead_code)] // VM mirror; consumed by the menu and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetItemVm {
    pub id: String,
    pub prefix: String,
    pub description_key: String,
    pub template: String,
}

/// The seven `BUILTIN_SQL_SNIPPETS` in React order, verbatim ids/prefixes/keys
/// and templates (the trailing `${n}` tabstop is part of the template).
#[allow(dead_code)] // pure projection; consumed by the menu and tests.
pub fn builtin_snippets() -> Vec<SnippetItemVm> {
    vec![
        snippet(
            "select-all",
            "sel*",
            "query.editor.snippet.selectAll",
            "SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}",
        ),
        snippet(
            "select-columns",
            "selc",
            "query.editor.snippet.selectColumns",
            "SELECT ${2:columns}\nFROM ${1:table_name};${3}",
        ),
        snippet(
            "insert",
            "ins",
            "query.editor.snippet.insert",
            "INSERT INTO ${1:table_name} (${2:columns})\nVALUES (${3:values});${4}",
        ),
        snippet(
            "update",
            "upd",
            "query.editor.snippet.update",
            "UPDATE ${1:table_name}\nSET ${2:column} = ${3:value}\nWHERE ${4:condition};${5}",
        ),
        snippet(
            "delete",
            "del",
            "query.editor.snippet.delete",
            "DELETE FROM ${1:table_name}\nWHERE ${2:condition};${3}",
        ),
        snippet(
            "join",
            "join",
            "query.editor.snippet.join",
            "JOIN ${1:table_name} ON ${1:table_name}.${2:id} = ${3:other_table}.${4:fk_id}${5}",
        ),
        snippet(
            "count",
            "count",
            "query.editor.snippet.count",
            "SELECT COUNT(1) AS total\nFROM ${1:table_name};${2}",
        ),
    ]
}

fn snippet(id: &str, prefix: &str, description_key: &str, template: &str) -> SnippetItemVm {
    SnippetItemVm {
        id: id.to_owned(),
        prefix: prefix.to_owned(),
        description_key: description_key.to_owned(),
        template: template.to_owned(),
    }
}

/// Host-owned projection of the props plus `settings.sqlSnippets` and the popup
/// state the native menu owns in React.
#[allow(dead_code)] // VM surface; owned by the query toolbar host.
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
    #[allow(dead_code)] // pure projection; consumed by the menu and tests.
    pub fn menu_items(&self) -> Vec<SnippetItemVm> {
        let mut items = builtin_snippets();
        items.extend(self.user_snippets.iter().cloned());
        items
    }
}

/// The host intents this control can emit.
#[allow(dead_code)] // host port; remove when the query toolbar host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnippetAction {
    ToggleMenu,
    Insert { id: String, template: String },
}

type ActionFn<S> = Rc<dyn Fn(&mut S, SnippetAction) + 'static>;

fn emit<S: 'static>(
    on_action: &Option<ActionFn<S>>,
    action: SnippetAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the toolbar trigger and its optional dropdown.
#[allow(dead_code)] // consumer: the query toolbar host (not landed).
pub fn build<S: 'static>(
    vm: &SnippetMenuButtonVm,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let label = t(LABEL_KEY).to_owned();
    let mut trigger = ToolbarButton::new(TRIGGER_KEY, label.clone())
        .icon(&ICON_TERMINAL)
        .icon_only(vm.icon_only)
        .variant(ToolbarButtonVariant::Ghost)
        .disabled(vm.disabled);
    if let Some(callback) = on_action.clone() {
        trigger = trigger.on_press(move |s: &mut S| callback(s, SnippetAction::ToggleMenu));
    }
    let trigger = trigger.build(dark).key(TRIGGER_KEY).label(label).help(t(TITLE_KEY).to_owned());

    let mut root = col().key("snippet-menu-button").gap(4.0).child(trigger);
    if vm.open {
        let mut menu = col()
            .key("snippet-menu")
            .role("menu")
            .label(t(LABEL_KEY))
            .gap(2.0);
        for item in vm.menu_items() {
            let description = t(&item.description_key).to_owned();
            let id = item.id.clone();
            let shortcut = item.prefix.clone();
            let mut row_el = row()
                .key(format!("snippet-{id}"))
                .role("menuitem")
                .focusable(true)
                .label(description.clone())
                .gap(8.0)
                .child(
                    text(description)
                        .key(format!("snippet-{id}-label"))
                        .font(k.ty.text_small)
                        .color(k.c.text_base),
                )
                .child(
                    text(shortcut)
                        .key(format!("snippet-{id}-shortcut"))
                        .font(k.ty.mono)
                        .color(k.c.text_muted),
                );
            if let Some(on_action) = on_action.clone() {
                let template = item.template.clone();
                row_el = row_el.on_press(move |s: &mut S| {
                    on_action(s, SnippetAction::Insert {
                        id: id.clone(),
                        template: template.clone(),
                    })
                });
            }
            menu = menu.child(row_el);
        }
        root = root.child(menu);
    }
    root
}
