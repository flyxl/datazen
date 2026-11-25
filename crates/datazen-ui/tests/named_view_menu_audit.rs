//! T192 independent audit: `NamedViewMenu` state machine.
//!
//! Covers React-spec branches the coder's suite does not: the overwrite-confirm
//! path's takeover id, menu close on a successful apply, default exclusivity,
//! import refusal, keyboard commits, and many-view geometry. State is observed
//! through `step` effects and the rendered tree, never private helpers alone.

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::components::filter::{
    on_import, on_save, step, FilterLogic, MenuState, NamedView, NamedViewAction, NamedViewEffect,
    NamedViewMenu, NamedViewScope, NamedViewVm, ViewFilter, ViewSort, ViewState,
};

fn scope() -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-1".to_owned(),
        database: Some("app".to_owned()),
        schema: None,
        table: "orders".to_owned(),
    }
}

fn filter(column: &str, operator: &str) -> ViewFilter {
    ViewFilter {
        column: column.to_owned(),
        operator: operator.to_owned(),
    }
}

fn view(id: &str, name: &str) -> NamedView {
    NamedView {
        id: id.to_owned(),
        name: name.to_owned(),
        scope: scope(),
        filters: vec![filter("id", "eq")],
        filter_logic: FilterLogic::And,
        sorts: vec![ViewSort {
            column: "id".to_owned(),
            descending: false,
        }],
        visible_columns: Some(vec!["id".to_owned()]),
        is_default: false,
    }
}

fn vm(views: Vec<NamedView>) -> NamedViewVm {
    NamedViewVm {
        state: MenuState::List,
        scope: scope(),
        views,
        error: String::new(),
        notice: String::new(),
        name: TextEdit::new(String::new()),
        renaming_id: None,
        rename_value: TextEdit::new(String::new()),
        columns: vec!["id".to_owned(), "name".to_owned()],
        dirty: false,
        applied: ViewState {
            filters: vec![filter("id", "eq")],
            filter_logic: FilterLogic::And,
            sorts: Vec::new(),
            visible_columns: None,
        },
        draft: None,
    }
}

/// React `handleRename`: on an overwrite conflict the renamed view is saved
/// with the CONFLICTING view's id and the original id is removed, leaving one
/// view of that name. The port must carry the conflict id through `Confirm`.
#[test]
fn overwrite_confirm_takes_over_the_conflicting_id() {
    let mut state = vm(vec![view("v1", "First"), view("v2", "Second")]);
    assert!(step(&mut state, NamedViewAction::StartRename("v1".to_owned())).is_empty());
    state.rename_value.replace_text("Second".to_owned());
    let asked = step(&mut state, NamedViewAction::CommitRename("v1".to_owned()));
    assert!(matches!(asked.last(), Some(NamedViewEffect::ConfirmOverwrite { .. })));

    let done = step(&mut state, NamedViewAction::Confirm("@rename:v1".to_owned()));
    assert_eq!(done.len(), 2, "confirm saves the takeover then removes the old row");
    match &done[0] {
        NamedViewEffect::Save(record) => assert_eq!(
            record.id, "v2",
            "the renamed view must take over the conflicting view's id, got {:?}",
            record.id
        ),
        other => panic!("first effect must be Save, got {other:?}"),
    }
    assert_eq!(done[1], NamedViewEffect::Remove("v1".to_owned()));
}

/// React `handleApply` calls `close()` after a successful apply; the panel
/// returns to idle. The port must not leave the list open.
#[test]
fn successful_apply_closes_the_menu() {
    let mut state = vm(vec![view("v1", "First")]);
    let effects = step(&mut state, NamedViewAction::Apply("v1".to_owned()));
    assert_eq!(effects, vec![NamedViewEffect::Apply(view("v1", "First"))]);
    assert_eq!(
        state.state,
        MenuState::Idle,
        "React closes the menu after applying; got {:?}",
        state.state
    );
}

/// Setting a view as default must leave exactly one default in the scope
/// (React `handleSetDefault` clears the flag on every other view in scope).
#[test]
fn on_save_of_a_default_clears_other_defaults() {
    let mut first = view("v1", "One");
    first.is_default = true;
    let mut state = vm(vec![first, view("v2", "Two")]);

    let mut second = view("v2", "Two");
    second.is_default = true;
    on_save(&mut state, second);

    let defaults: Vec<&str> = state
        .views
        .iter()
        .filter(|entry| entry.is_default)
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(defaults, vec!["v2"], "setting v2 default must clear v1");
}

/// An import carrying an operator the host does not know is refused whole:
/// no Save effects, an error state and a localized message.
#[test]
fn import_refuses_unknown_operators() {
    let mut state = vm(Vec::new());
    let mut incoming = view("other", "Imported");
    incoming.filters = vec![filter("id", "noSuchOp")];
    let effects = on_import(&mut state, vec![incoming]);
    assert!(effects.is_empty(), "no partial import");
    assert_eq!(state.state, MenuState::Error);
    assert!(
        state.error.contains("noSuchOp"),
        "error reports the unsupported operator: {:?}",
        state.error
    );
}

// ---- DOM-level keyboard paths -------------------------------------------------------------------

struct Probe {
    dark: bool,
    vm: NamedViewVm,
    actions: Vec<NamedViewAction>,
}

fn view_fn(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    NamedViewMenu::<Probe>::new(&s.vm)
        .on_name_edit(|st: &mut Probe| &mut st.vm.name)
        .on_rename_edit(|st: &mut Probe| &mut st.vm.rename_value)
        .on_action(|st: &mut Probe, action| st.actions.push(action))
        .build(s.dark)
}

fn runtime(views: Vec<NamedView>) -> Runtime<Probe> {
    let mut p = Probe {
        dark: true,
        vm: vm(views),
        actions: Vec::new(),
    };
    p.vm.state = MenuState::List;
    let mut rt = Runtime::new(
        p,
        view_fn,
        (1440.0, 816.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(0.0);
}

#[test]
fn enter_in_name_field_commits_save() {
    let mut rt = runtime(Vec::new());
    let effects = step(&mut rt.state.vm, NamedViewAction::SaveAs);
    assert!(effects.is_empty());
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    click(&mut rt, "named-view-name-input");
    rt.paste("My view");
    rt.frame(0.0);
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::Save));
}

#[test]
fn escape_in_rename_field_cancels() {
    let mut rt = runtime(vec![view("v1", "First")]);
    let _ = step(&mut rt.state.vm, NamedViewAction::StartRename("v1".to_owned()));
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    click(&mut rt, "named-view-rename-input");
    press(&mut rt, Key::Escape);
    assert_eq!(rt.state.actions.last(), Some(&NamedViewAction::CancelRename));
}

/// Rect-based geometry with many long-named views, both windows/palettes.
#[test]
fn many_views_stay_inside_both_windows_both_palettes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let views: Vec<NamedView> = (0..12)
                .map(|i| view(&format!("v{i}"), &format!("A very long saved view name number {i}")))
                .collect();
            let mut p = Probe {
                dark,
                vm: vm(views),
                actions: Vec::new(),
            };
            p.vm.state = MenuState::List;
            let mut rt = Runtime::new(
                p,
                view_fn,
                (w, h),
                Env {
                    system_dark: dark,
                    scale: 1.0,
                },
            );
            rt.frame(0.0);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
                    outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(
                outside.is_empty(),
                "many views dark={dark} {w}x{h}: {} node(s) outside: {outside:?}",
                outside.len()
            );
        }
    }
}
