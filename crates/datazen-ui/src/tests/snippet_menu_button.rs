//! T516: `SnippetMenuButton` port — the React toolbar journey pinning the
//! trigger key/label/help, the seven exact built-in items with prefixes and
//! templates, user snippets, host insert actions, and rect geometry at both
//! sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::toolbar::snippet_menu_button::{
    build, builtin_snippets, SnippetAction, SnippetItemVm, SnippetMenuButtonVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: SnippetMenuButtonVm,
    actions: Rc<RefCell<Vec<SnippetAction>>>,
    dark: bool,
}

fn scene(vm: SnippetMenuButtonVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            SnippetAction::ToggleMenu => state.vm.open = !state.vm.open,
            SnippetAction::Insert { .. } => state.vm.open = false,
        }
        actions.borrow_mut().push(action);
    });
    let mounted = build(&s.vm, Some(callback), s.dark);
    col().w_full().h_full().child(mounted)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

#[test]
fn builtins_match_the_react_library_exactly() {
    let items = builtin_snippets();
    let expected: [(&str, &str, &str); 7] = [
        ("select-all", "sel*", "query.editor.snippet.selectAll"),
        ("select-columns", "selc", "query.editor.snippet.selectColumns"),
        ("insert", "ins", "query.editor.snippet.insert"),
        ("update", "upd", "query.editor.snippet.update"),
        ("delete", "del", "query.editor.snippet.delete"),
        ("join", "join", "query.editor.snippet.join"),
        ("count", "count", "query.editor.snippet.count"),
    ];
    assert_eq!(items.len(), expected.len());
    for (item, (id, prefix, key)) in items.iter().zip(expected) {
        assert_eq!(item.id, id, "builtin id order");
        assert_eq!(item.prefix, prefix, "builtin shortcut");
        assert_eq!(item.description_key, key, "builtin description key");
        assert!(item.template.ends_with('}'), "every template ends with a terminal tabstop");
    }
    assert_eq!(items[0].template, "SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}");
    assert_eq!(items[6].template, "SELECT COUNT(1) AS total\nFROM ${1:table_name};${2}");
}

#[test]
fn trigger_keys_and_label_are_exact_and_menu_starts_closed() {
    let rt = runtime(scene(SnippetMenuButtonVm::default(), true), DESIGN);
    assert!(has_key(&rt, "snippet-menu-button"));
    assert!(has_key(&rt, "editor-snippets-button"), "React's tid() is kept");
    assert_eq!(
        element(&rt, "editor-snippets-button").label.as_deref(),
        Some(t("query.snippets"))
    );
    assert!(!has_key(&rt, "snippet-menu"), "the native menu is closed initially");
}

#[test]
fn pressing_the_trigger_emits_toggle_and_opens_all_builtin_items() {
    let mut rt = runtime(scene(SnippetMenuButtonVm::default(), true), DESIGN);
    click(&mut rt, "editor-snippets-button");
    assert_eq!(*rt.state.actions.borrow(), [SnippetAction::ToggleMenu]);
    assert!(has_key(&rt, "snippet-menu"));
    for item in builtin_snippets() {
        let key = format!("snippet-{}", item.id);
        assert!(has_key(&rt, &key), "{key} is mounted");
        assert_eq!(element(&rt, &key).label.as_deref(), Some(t(&item.description_key)));
        assert_eq!(text_of(&rt, &format!("{key}-shortcut")), item.prefix);
    }
}

#[test]
fn inserting_a_builtin_reports_its_id_and_template_to_the_host() {
    let mut rt = runtime(scene(SnippetMenuButtonVm::default(), true), DESIGN);
    click(&mut rt, "editor-snippets-button");
    click(&mut rt, "snippet-select-all");
    assert_eq!(
        *rt.state.actions.borrow(),
        [
            SnippetAction::ToggleMenu,
            SnippetAction::Insert {
                id: "select-all".to_owned(),
                template: "SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}".to_owned(),
            },
        ],
        "the host receives the built-in template, not a fake editor write"
    );
}

#[test]
fn user_snippets_append_and_insert_through_the_same_port() {
    let vm = SnippetMenuButtonVm {
        user_snippets: vec![SnippetItemVm {
            id: "mine".to_owned(),
            prefix: "mine*".to_owned(),
            description_key: "query.editor.snippet.count".to_owned(),
            template: "SELECT 1;".to_owned(),
        }],
        ..SnippetMenuButtonVm::default()
    };
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, "editor-snippets-button");
    assert!(has_key(&rt, "snippet-mine"), "user snippets follow the built-ins");
    assert_eq!(text_of(&rt, "snippet-mine-shortcut"), "mine*");
    click(&mut rt, "snippet-mine");
    assert_eq!(
        rt.state.actions.borrow().last(),
        Some(&SnippetAction::Insert {
            id: "mine".to_owned(),
            template: "SELECT 1;".to_owned(),
        })
    );
}

#[test]
fn disabled_trigger_is_inert() {
    let vm = SnippetMenuButtonVm {
        disabled: true,
        ..SnippetMenuButtonVm::default()
    };
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(!element(&rt, "editor-snippets-button").focusable, "disabled trigger");
    click(&mut rt, "editor-snippets-button");
    assert!(rt.state.actions.borrow().is_empty(), "a disabled control emits no action");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = SnippetMenuButtonVm {
                icon_only: true,
                ..SnippetMenuButtonVm::default()
            };
            let rt = runtime(scene(vm.clone(), dark), size);
            assert_inside(&rt, size, &format!("closed {size:?} dark={dark}"));

            vm.open = true;
            vm.user_snippets.push(SnippetItemVm {
                id: "mine".to_owned(),
                prefix: "mine*".to_owned(),
                description_key: "query.editor.snippet.count".to_owned(),
                template: "SELECT 1;".to_owned(),
            });
            let rt = runtime(scene(vm, dark), size);
            assert_inside(&rt, size, &format!("open {size:?} dark={dark}"));
        }
    }
}

// ── T516 independent audit (appended; reuses this module's Scene) ────────

/// React passes `title={t('query.snippetsTitle')}` as the toolbar button's
/// tooltip (`SnippetMenuButton.tsx:59`). The port maps it to `.help(...)`; that
/// exact text is untested by the coder suite.
#[test]
fn audit_trigger_tooltip_is_the_snippets_title() {
    let rt = runtime(scene(SnippetMenuButtonVm::default(), true), DESIGN);
    assert_eq!(
        element(&rt, "editor-snippets-button").help.as_deref(),
        Some(t("query.snippetsTitle")),
        "React's title becomes the button help/tooltip"
    );
}

/// React's `iconOnly` trigger keeps the name as aria-label while hiding the
/// visible text (`SnippetMenuButton.tsx:24,58`). The port must keep the label on
/// the button element even when icon-only.
#[test]
fn audit_icon_only_trigger_keeps_the_aria_label() {
    let vm = SnippetMenuButtonVm {
        icon_only: true,
        ..SnippetMenuButtonVm::default()
    };
    let rt = runtime(scene(vm, true), DESIGN);
    assert_eq!(
        element(&rt, "editor-snippets-button").label.as_deref(),
        Some(t("query.snippets")),
        "icon-only still announces the snippet name"
    );
}

/// React's menu comes from the app-wide native context menu: a menu whose items
/// are clickable (`SnippetMenuButton.tsx:38-49`). The port models it as an
/// inline `role=menu`/`role=menuitem` tree; those roles are the a11y channel and
/// were not asserted.
#[test]
fn audit_menu_and_items_expose_menu_roles() {
    let mut rt = runtime(scene(SnippetMenuButtonVm::default(), true), DESIGN);
    click(&mut rt, "editor-snippets-button");
    assert_eq!(element(&rt, "snippet-menu").role, "menu");
    for item in builtin_snippets() {
        let key = format!("snippet-{}", item.id);
        assert_eq!(element(&rt, &key).role, "menuitem", "{key} is a menuitem");
    }
}

/// With no injected host callback the menu still renders its items, but
/// pressing one emits nothing (the `if let Some(on_action)` branch in `build`).
#[test]
fn audit_no_host_port_renders_items_but_emits_nothing() {
    // Build directly with no callback — this module's `view()` always wires one.
    fn no_host_view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
        build::<Scene>(&s.vm, None, s.dark)
    }
    let mut vm = SnippetMenuButtonVm::default();
    vm.open = true;
    let scene = scene(vm, true);
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, no_host_view, DESIGN, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    assert!(has_key(&rt, "snippet-menu"));
    assert!(has_key(&rt, "snippet-select-all"));
    let (x, y) = rt.find("snippet-select-all").unwrap_or_else(|| panic!("item laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert!(rt.state.actions.borrow().is_empty(), "no host port means no emitted action");
}
