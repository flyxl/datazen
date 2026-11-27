//! The mounted navigator-to-search flow, with a controllable adapter so a late
//! answer and a reconnect to the same ID can be delivered deterministically.
use std::{cell::RefCell, rc::Rc};

use datazen_ui::test_support::*;
use rune::{Ime, Key, KeyEvent, Mods};

const ENTRY: &str = "navigator-search-values";
const DIALOG: &str = "database-value-search-dialog";

type Search = (u32, NativeValueSearchScope, String, String);

struct SearchAdapter {
    scope: Rc<RefCell<Option<NativeValueSearchScope>>>,
    jobs: Rc<RefCell<Vec<Search>>>,
}

impl DesktopAdapter for SearchAdapter {
    fn native_value_search_scope(&self) -> Option<NativeValueSearchScope> {
        self.scope.borrow().clone()
    }

    fn begin_scoped_value_search(
        &mut self,
        generation: u32,
        scope: NativeValueSearchScope,
        value: String,
        mode: String,
    ) -> bool {
        assert_eq!(self.scope.borrow().as_ref(), Some(&scope));
        self.jobs.borrow_mut().push((generation, scope, value, mode));
        true
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

fn press(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
}

fn open_fixture(rt: &mut Runtime<AppState>, scope: &NativeValueSearchScope) {
    rt.state.deliver_host_job(AdapterDelivery::ConnectionOpened {
        generation: scope.session_generation,
        id: scope.connection_id.clone(),
        catalog: Ok(Catalog {
            databases: vec![DatabaseNode {
                name: scope.database.clone(),
                schemas: vec![SchemaNode {
                    name: scope.database.clone(),
                    tables: vec!["users".into()],
                    views: Vec::new(),
                }],
            }],
        }),
    });
    rt.invalidate();
    rt.frame(0.0);
}

fn answer(generation: u32) -> AdapterDelivery {
    AdapterDelivery::ValueSearched {
        generation,
        hits: Ok(vec![ValueSearchHit {
            table: "users".into(),
            column: "name".into(),
            matches: 1,
        }]),
    }
}

#[test]
fn connected_search_uses_the_navigator_scope_and_rejects_closed_or_replaced_results() {
    let mut rt = runtime();
    let jobs = Rc::new(RefCell::new(Vec::new()));
    let host_scope = Rc::new(RefCell::new(None));
    rt.state.desktop = Box::new(SearchAdapter {
        scope: host_scope.clone(),
        jobs: jobs.clone(),
    });
    rt.frame(0.0);
    assert!(rt.find(ENTRY).is_none(), "no session, no action");

    let mut scope = NativeValueSearchScope {
        connection_id: "profile-a".into(),
        database: "main".into(),
        session_generation: rt.state.next_read_generation(),
    };
    *host_scope.borrow_mut() = Some(scope.clone());
    open_fixture(&mut rt, &scope);
    // A different database stored on a SQL tab must not become navigator scope.
    let tab = rt.state.open_query_tab("Second database").expect("query tab");
    rt.state.query_database_targets.insert(
        tab,
        NativeQueryDatabase {
            connection_id: scope.connection_id.clone(),
            database: "other-db".into(),
        },
    );
    rt.invalidate();
    rt.frame(0.0);

    press(&mut rt, ENTRY);
    assert!(rt.find(DIALOG).is_some());
    assert!(jobs.borrow().is_empty());
    press(&mut rt, "value-search-input");
    rt.ime(Ime::Commit("user-42".into()));
    rt.frame(0.0);
    press(&mut rt, "value-search-start");
    let first = jobs.borrow()[0].clone();
    assert_eq!(first.1, scope);
    assert_eq!((&*first.2, &*first.3), ("user-42", "contains"));

    // Closing via Escape leaves the SQL tab as it was and drops late results.
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    rt.frame(0.0);
    assert!(rt.find(DIALOG).is_none());
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(tab));
    rt.state.deliver_host_job(answer(first.0));
    assert!(rt.state.value_search_hits.is_empty());

    press(&mut rt, ENTRY);
    assert!(rt.state.value_search_value.text.is_empty());
    press(&mut rt, "value-search-input");
    rt.ime(Ime::Commit("user-43".into()));
    rt.frame(0.0);
    press(&mut rt, "value-search-start");
    let second = jobs.borrow()[1].0;

    // A reconnect to the SAME profile must not let an answer from the
    // old worker session populate the UI or resolve a result in the new one.
    scope.session_generation = rt.state.next_read_generation();
    *host_scope.borrow_mut() = Some(scope.clone());
    rt.state.deliver_host_job(answer(second));
    assert!(rt.state.navigator_value_search_scope.is_none());
    assert!(rt.state.value_search_hits.is_empty());
    rt.invalidate();
    rt.frame(0.0);
    assert!(rt.find(ENTRY).is_none(), "new session not yet accepted by UI");

    open_fixture(&mut rt, &scope);
    press(&mut rt, ENTRY);
    press(&mut rt, "value-search-input");
    rt.ime(Ime::Commit("user-44".into()));
    rt.frame(0.0);
    press(&mut rt, "value-search-start");
    let third = jobs.borrow()[2].0;
    rt.state.deliver_host_job(answer(third));
    rt.invalidate();
    rt.frame(0.0);
    assert!(rt.find("value-hit-0").is_some());
    press(&mut rt, "value-hit-0");
    assert!(rt.find(DIALOG).is_none());
    let filter = rt.state.filter.as_ref().expect("real locate applied a filter");
    assert_eq!((&*filter.column, &*filter.contains), ("name", "user-44"));

    rt.state.disconnect();
    rt.invalidate();
    rt.frame(0.0);
    assert!(rt.find(ENTRY).is_none(), "disconnect hides search again");
}
