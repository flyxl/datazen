use super::*;
use datazen_ui_api::{DatabaseNode, SchemaNode};

fn catalog(colliding: bool) -> Catalog {
    Catalog { databases: vec![DatabaseNode {
        name: "db".into(),
        schemas: vec![
            SchemaNode {
                name: "a".into(),
                tables: vec!["customers".into(), "orders".into(), "dupe".into()],
                views: vec!["recent".into()],
            },
            SchemaNode {
                name: "b".into(),
                tables: if colliding { vec!["customers".into()] } else { Vec::new() },
                views: vec!["dupe".into()],
            },
        ],
    }] }
}

fn live_state() -> AppState {
    let mut state = AppState::default();
    state.active = Section::Workspace;
    state.desktop_connection = Some("live".into());
    state.desktop_catalog = Some(catalog(false));
    state.navigator_value_search_session_generation = Some(7);
    state
}

fn find(state: &AppState, name: &str) -> ObjectSearchResult {
    let vm = GlobalObjectSearchVm {
        open: true,
        index: state.native_object_search.index.clone(),
        query: TextEdit::new(name),
    };
    vm.results().into_iter().find(|result| result.name == name).unwrap()
}

#[test]
fn native_scope_lists_real_tables_and_views_and_omits_ambiguous_names() {
    let mut state = live_state();
    state.open_native_object_search();
    assert!(state.native_object_search_is_current());
    let result = find(&state, "recent");
    assert_eq!(result.object_type, ObjectType::View);
    assert_eq!(result.database.as_deref(), Some("db"));
    assert_eq!(result.schema.as_deref(), Some("a"));
    assert!(state.native_object_search.index.iter().all(|entry|
        entry.relations.iter().all(|relation| relation.name != "dupe")));
    assert!(unique_open_target(state.desktop_catalog.as_ref().unwrap(), &result));
}

#[test]
fn native_scope_rejects_queued_ambiguous_forged_and_stale_callbacks() {
    let mut state = live_state();
    state.open_native_object_search();
    let epoch = state.native_object_search.epoch;
    let result = find(&state, "customers");
    let mut forged = result.clone();
    forged.schema = Some("b".into());
    state.native_object_search_action(epoch, ObjectSearchAction::OpenResult(forged));
    assert!(state.tabs.selected().is_none());

    // A formerly unique row became ambiguous while the callback was queued.
    state.desktop_catalog = Some(catalog(true));
    state.native_object_search_action(epoch, ObjectSearchAction::OpenResult(result.clone()));
    assert!(state.tabs.selected().is_none());

    state.desktop_catalog = Some(catalog(false));
    state.navigator_value_search_session_generation = Some(8);
    state.native_object_search_action(epoch, ObjectSearchAction::OpenResult(result.clone()));
    assert!(state.tabs.selected().is_none());
    assert!(!state.native_object_search_is_current());

    state.open_native_object_search();
    state.native_object_search_action(epoch, ObjectSearchAction::OpenResult(result));
    assert!(state.tabs.selected().is_none());
    assert!(state.native_object_search_is_current());
    state.disconnect();
    assert!(!state.native_object_search_is_available());
    assert!(!state.native_object_search_is_current());
}
