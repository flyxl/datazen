//! T480: `ObjectBrowser` — kind filter tabs, object rows and identity-based
//! selection, both context menus, controlled DDL/execute flow, empty/loading/
//! error branches and rect geometry at both window sizes in both themes. Every
//! command is recorded as an [`ObjectBrowserAction`]; no fake IPC or clipboard.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::object_browser::{
    build, identity_key, same_object_identity, BrowserKind, BrowserObject, ObjectBrowserAction,
    ObjectBrowserActionFn, ObjectBrowserVm, EDITOR_MENU_KEY, EDITOR_KEY, ITEM_KEY, LIST_KEY,
    LIST_MENU_KEY, ROOT_KEY, TAB_PREFIX,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ObjectBrowserVm,
    actions: Rc<RefCell<Vec<ObjectBrowserAction>>>,
    dark: bool,
    no_port: bool,
}

fn object(name: &str, schema: Option<&str>, signature: Option<&str>) -> BrowserObject {
    BrowserObject {
        kind: BrowserKind::Function,
        schema: schema.map(str::to_owned),
        name: name.to_owned(),
        signature: signature.map(str::to_owned),
        target_schema: None,
        target_name: None,
    }
}

fn scene(vm: ObjectBrowserVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
        no_port: false,
    }
}

fn listed_vm() -> ObjectBrowserVm {
    ObjectBrowserVm {
        objects: vec![
            object("fn_ok", Some("public"), None),
            object("lookup", Some("public"), Some("integer")),
            object("lookup", Some("public"), Some("text")),
        ],
        ddl: TextEdit::new("-- empty".to_owned()),
        ..Default::default()
    }
}

fn action_name(action: &ObjectBrowserAction) -> &'static str {
    match action {
        ObjectBrowserAction::SetKind(_) => "set-kind",
        ObjectBrowserAction::Load { .. } => "load",
        ObjectBrowserAction::Refresh { .. } => "refresh",
        ObjectBrowserAction::OpenObject { .. } => "open-object",
        ObjectBrowserAction::CopyName { .. } => "copy-name",
        ObjectBrowserAction::CopyDdl { .. } => "copy-ddl",
        ObjectBrowserAction::Execute { .. } => "execute",
        ObjectBrowserAction::FormatDdl => "format",
        ObjectBrowserAction::ToggleComment => "comment",
        ObjectBrowserAction::ListContextMenu { .. } => "list-menu",
        ObjectBrowserAction::EditorContextMenu { .. } => "editor-menu",
        ObjectBrowserAction::CloseMenu => "close-menu",
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let on_action: Option<ObjectBrowserActionFn<Scene>> = if s.no_port {
        None
    } else {
        let actions = s.actions.clone();
        Some(Rc::new(move |state: &mut Scene, action| {
            state.vm.reduce(&action);
            actions.borrow_mut().push(action);
        }))
    };
    build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn names(rt: &Runtime<Scene>) -> Vec<String> {
    rt.state
        .actions
        .borrow()
        .iter()
        .map(action_name)
        .map(str::to_owned)
        .collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn right_click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
}

#[test]
fn identity_keys_keep_overloaded_routines_apart() {
    let integer = object("lookup", Some("public"), Some("integer"));
    let text = object("lookup", Some("public"), Some("text"));
    assert_ne!(identity_key(&integer), identity_key(&text));
    assert_eq!(
        identity_key(&object("fn_ok", Some("public"), None)),
        "[\"function\",\"public\",\"fn_ok\",null,null,null]"
    );
    assert!(same_object_identity(Some(&integer), &integer.clone()));
    assert!(!same_object_identity(Some(&integer), &text));
}

#[test]
fn tabs_and_rows_pin_the_react_testids_and_labels() {
    let rt = runtime(scene(listed_vm(), true), DESIGN);
    for key in [
        ROOT_KEY,
        "object-browser-aside",
        "object-browser-tabs",
        LIST_KEY,
        "object-browser-detail",
        ITEM_KEY,
        "object-browser-execute",
    ] {
        assert!(has_key(&rt, key), "{key} is composed");
    }
    for kind in BrowserKind::ALL {
        let key = format!("{TAB_PREFIX}{}", kind.as_str());
        assert!(has_key(&rt, &key), "the {key} tab mounts");
        assert_eq!(label_of(&rt, &key), kind.label(), "the tab label uses React's key");
    }
    assert_eq!(text_of(&rt, "object-browser-count"), "3");
    assert_eq!(text_of(&rt, "object-browser-empty"), "", "no empty state with rows");
    for at in 0..3 {
        assert!(has_key(&rt, &format!("{ITEM_KEY}-{at}")), "one row per object");
    }
    assert_eq!(text_of(&rt, "object-browser-item-name-0"), "fn_ok");
    assert_eq!(text_of(&rt, "object-browser-item-schema-0"), "public");
    assert_eq!(
        text_of(&rt, "object-browser-selection"),
        t("objects.pick"),
        "the panel asks for a pick before selection"
    );
    assert_eq!(label_of(&rt, "object-browser-execute"), t("query.execute"));
}

#[test]
fn selection_opens_ddl_and_execute_reaches_the_host() {
    let mut rt = runtime(scene(listed_vm(), false), DESIGN);
    rt.frame(0.0);
    click(&mut rt, &format!("{ITEM_KEY}-1"));
    assert_eq!(names(&rt).first().map(String::as_str), Some("open-object"));
    assert_eq!(
        rt.state.vm.selected.as_ref().map(|o| o.name.as_str()),
        Some("lookup"),
        "clicking a row selects that object"
    );
    // A second overload is independently selectable and replaces the first.
    click(&mut rt, &format!("{ITEM_KEY}-2"));
    assert_eq!(
        rt.state.vm.selected.as_ref().and_then(|o| o.signature.as_deref()),
        Some("text"),
        "overloads keep their own identity"
    );
    assert_eq!(text_of(&rt, "object-browser-selection"), "function · lookup");

    click(&mut rt, "object-browser-execute");
    assert!(
        rt.state.actions.borrow().iter().any(|a| matches!(
            a,
            ObjectBrowserAction::Execute { sql, schema }
                if sql == "-- empty" && schema.as_deref() == Some("public")
        )),
        "execute forwards the DDL and the selected schema"
    );
}

#[test]
fn execute_is_disabled_without_ddl() {
    let mut vm = listed_vm();
    vm.ddl = TextEdit::new(String::new());
    let mut rt = runtime(scene(vm, true), MIN);
    click(&mut rt, "object-browser-execute");
    assert!(
        !names(&rt).iter().any(|n| n == "execute"),
        "an empty editor cannot execute"
    );
}

#[test]
fn context_menu_entries_emit_their_react_actions() {
    let mut rt = runtime(scene(listed_vm(), true), DESIGN);
    right_click(&mut rt, &format!("{ITEM_KEY}-0"));
    assert!(has_key(&rt, &format!("{LIST_MENU_KEY}-menu")), "list menu opens");
    assert_eq!(label_of(&rt, &format!("{LIST_MENU_KEY}-item0")), t("objects.refresh"));
    assert_eq!(label_of(&rt, &format!("{LIST_MENU_KEY}-item3")), t("common.copyDdl"));
    click(&mut rt, &format!("{LIST_MENU_KEY}-item1"));
    assert!(names(&rt).iter().any(|n| n == "open-object"), "Open entry fired");

    right_click(&mut rt, EDITOR_KEY);
    assert!(has_key(&rt, &format!("{EDITOR_MENU_KEY}-menu")), "editor menu opens");
    // Entry 0 is the separator, so Execute is item1.
    assert_eq!(label_of(&rt, &format!("{EDITOR_MENU_KEY}-item1")), t("query.execute"));
    assert_eq!(label_of(&rt, &format!("{EDITOR_MENU_KEY}-item3")), t("query.comment"));
    click(&mut rt, &format!("{EDITOR_MENU_KEY}-item2"));
    assert!(names(&rt).iter().any(|n| n == "format"), "Format entry fired");
}

#[test]
fn kind_tabs_filter_and_clear_the_selection() {
    let mut vm = listed_vm();
    vm.selected = Some(object("fn_ok", Some("public"), None));
    let mut rt = runtime(scene(vm, false), DESIGN);
    click(&mut rt, &format!("{TAB_PREFIX}procedure"));
    assert_eq!(rt.state.vm.kind, BrowserKind::Procedure);
    assert!(rt.state.vm.selected.is_none(), "switching kind clears the selection");
    assert_eq!(rt.state.vm.ddl.text, "", "switching kind clears the DDL");
    assert_eq!(names(&rt).first().map(String::as_str), Some("set-kind"));
}

#[test]
fn empty_loading_and_error_states_pin_their_keys() {
    let rt = runtime(scene(ObjectBrowserVm::default(), true), MIN);
    assert!(has_key(&rt, "object-browser-empty"));
    assert_eq!(text_of(&rt, "object-browser-empty-text"), t("objects.empty"));

    let mut loading = ObjectBrowserVm::default();
    loading.loading = true;
    let rt = runtime(scene(loading, false), MIN);
    assert!(has_key(&rt, "object-browser-loading"));
    assert_eq!(text_of(&rt, "object-browser-loading-text"), t("common.loading"));

    let mut failed = ObjectBrowserVm::default();
    failed.error = "boom".to_owned();
    let rt = runtime(scene(failed, true), MIN);
    assert!(has_key(&rt, "object-browser-error"));
    assert!(!has_key(&rt, "object-browser-empty"), "an error replaces the empty state");
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut rt = runtime(scene(listed_vm(), dark), (width, height));
            rt.frame(0.0);
            for phase in 0..3 {
                if phase == 1 {
                    right_click(&mut rt, &format!("{ITEM_KEY}-0"));
                } else if phase == 2 {
                    click(&mut rt, &format!("{LIST_MENU_KEY}-item0"));
                    right_click(&mut rt, EDITOR_KEY);
                }
                for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, w, h] = element.rect;
                    assert!(
                        x >= -0.5 && x + w <= width + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows horizontally",
                        element.key,
                        element.rect
                    );
                    assert!(
                        y >= -0.5 && y + h <= height + 0.5,
                        "{width}x{height} dark={dark} phase={phase}: {:?} {:?} overflows vertically",
                        element.key,
                        element.rect
                    );
                }
            }
        }
    }
}
