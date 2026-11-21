//! T470: `NavigatorTreeRow` port — 13 variants, ARIA equivalence, actions and
//! two-window/two-palette geometry. Observations come from the runtime tree and
//! from host-recorded actions.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::views::navigator::navigator_tree_row::{
    aria_expanded, aria_level_of, indent_of, is_decoration, is_navigator_branch,
    navigator_tree_row, searched, section_group_key, RowAction, RowConnectionStatus, RowKind,
    RowVm, TreeRowHostPort, PINNED_GROUP_KEY, RECENT_GROUP_KEY, TREE_TOP_LEVEL,
};

#[derive(Clone)]
struct Probe {
    events: Vec<RowAction>,
    row: RowVm,
    dark: bool,
}

fn row_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = TreeRowHostPort {
        on_action: Some(Rc::new(move |st: &mut Probe, action| st.events.push(action))),
    };
    col().w_full().h_full().child(navigator_tree_row(&s.row, s.dark, &host))
}

fn row_vm(kind: RowKind) -> RowVm {
    RowVm { kind, depth: 2, level_depth: 2 }
}

#[test]
fn aria_contract_covers_all_thirteen_variants() {
    let variants: Vec<RowVm> = vec![
        row_vm(RowKind::Section { section: "pinned".into(), display_name: "Pinned".into(), count: 1, expanded: true }),
        row_vm(RowKind::Group { group_name: "team-a".into(), display_name: "Team A".into(), count: 1, expanded: true, drop_target: false }),
        row_vm(RowKind::Connection {
            id: "c1".into(), name: "c1".into(), engine: "pg".into(), section_group: None,
            is_selected: true, status: RowConnectionStatus::Connected, expanded: true,
            match_reason: None, match_context: None, drop_before: false, drop_after: false,
        }),
        row_vm(RowKind::Db { connection_id: "c1".into(), db_session_id: "s1".into(), db_name: "db1".into(), expanded: true, loading: false, is_open: true }),
        row_vm(RowKind::Schema { connection_id: "c1".into(), db_name: "db1".into(), schema_name: "public".into(), expanded: true }),
        row_vm(RowKind::Category { key: "c1::db1::public::tables".into(), cat_id: "tables".into(), label_key: "schemaTree.tables".into(), expanded: true }),
        row_vm(RowKind::Table { name: "users".into(), schema: Some("public".into()), cat_id: "tables".into(), is_selected: false, connection_id: "c1".into(), db_session_id: "s1".into(), db_name: "db1".into() }),
        row_vm(RowKind::Object { name: "fn_calc".into(), schema: None, cat_id: "routines".into() }),
        row_vm(RowKind::KvDb { connection_id: "c2".into(), db_session_id: "s2".into(), db_name: "db0".into(), is_selected: false, count: None }),
        row_vm(RowKind::DbLoading { owner_key: "conn:c2".into() }),
        row_vm(RowKind::DbError { message: "denied".into() }),
        RowVm { kind: RowKind::NamespaceNode { name: "tbl".into(), expanded: false, is_leaf: false, key: "ns:db1::tbl".into(), connection_id: "c3".into(), db_session_id: "s3".into() }, depth: 2, level_depth: 2 },
        RowVm { kind: RowKind::EmptyGroup { group_name: Some("team-b".into()), drop_target: false }, depth: 1, level_depth: 1 },
        RowVm { kind: RowKind::NoConnections, depth: 0, level_depth: 0 },
    ];
    assert_eq!(variants.len(), 14, "13 types plus the polymorphic namespace leaf");
    assert_eq!(TREE_TOP_LEVEL, 1);
    for row in &variants {
        if is_decoration(row) {
            assert!(aria_expanded(row).is_none(), "decorations announce no disclosure");
        } else {
            assert_eq!(aria_level_of(row), row.level_depth + TREE_TOP_LEVEL);
        }
    }
    // Leaves own no child list: absent, never Some(false).
    for kind in [
        RowKind::Table { name: "t".into(), schema: None, cat_id: "tables".into(), is_selected: false, connection_id: "c".into(), db_session_id: "s".into(), db_name: "d".into() },
        RowKind::Object { name: "o".into(), schema: None, cat_id: "routines".into() },
        RowKind::KvDb { connection_id: "c".into(), db_session_id: "s".into(), db_name: "d".into(), is_selected: false, count: None },
        RowKind::DbLoading { owner_key: "k".into() },
    ] {
        let row = row_vm(kind);
        assert!(!is_navigator_branch(&row));
        assert!(aria_expanded(&row).is_none(), "leaf must not announce a disclosure");
    }
}

#[test]
fn connection_expansion_exception_and_polymorphic_namespace() {
    let mut conn = row_vm(RowKind::Connection {
        id: "c1".into(), name: "c1".into(), engine: "pg".into(), section_group: None,
        is_selected: false, status: RowConnectionStatus::Error, expanded: false,
        match_reason: None, match_context: None, drop_before: false, drop_after: false,
    });
    assert!(is_navigator_branch(&conn), "a database's parent stays a branch while idle");
    assert!(aria_expanded(&conn).is_none(), "idle connection announces no disclosure");

    if let RowKind::Connection { status, expanded, .. } = &mut conn.kind {
        *status = RowConnectionStatus::Connected;
        *expanded = true;
    }
    assert_eq!(aria_expanded(&conn), Some(true));

    let branch = row_vm(RowKind::NamespaceNode {
        name: "tbl".into(), expanded: false, is_leaf: false, key: "ns:db1::tbl".into(),
        connection_id: "c3".into(), db_session_id: "s3".into(),
    });
    assert!(is_navigator_branch(&branch));
    assert_eq!(aria_expanded(&branch), Some(false));
    let mut leaf = branch.clone();
    if let RowKind::NamespaceNode { is_leaf, .. } = &mut leaf.kind {
        *is_leaf = true;
    }
    assert!(!is_navigator_branch(&leaf));
    assert!(aria_expanded(&leaf).is_none());
}

#[test]
fn search_shifts_level_by_one_but_not_indent() {
    let row = row_vm(RowKind::Db {
        connection_id: "c1".into(), db_session_id: "s1".into(), db_name: "db1".into(),
        expanded: true, loading: false, is_open: false,
    });
    let shifted = searched(&row);
    assert_eq!(aria_level_of(&shifted), aria_level_of(&row) - 1);
    assert_eq!(indent_of(&shifted), indent_of(&row), "paint indent must not move");

    let root = RowVm {
        kind: RowKind::Group { group_name: "g".into(), display_name: "G".into(), count: 1, expanded: true, drop_target: false },
        depth: 0, level_depth: 0,
    };
    assert_eq!(aria_level_of(&searched(&root)), aria_level_of(&root), "roots are unaffected");
    assert_eq!(section_group_key("pinned"), PINNED_GROUP_KEY);
    assert_eq!(section_group_key("recent"), RECENT_GROUP_KEY);
}

#[test]
fn section_enter_and_click_toggle_the_group() {
    let row = row_vm(RowKind::Section {
        section: "pinned".into(), display_name: "Pinned".into(), count: 1, expanded: true,
    });
    let mut rt = Runtime::new(
        Probe { events: Vec::new(), row, dark: true },
        row_view,
        (1440.0, 816.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    // The row is a tab stop; focus it with a press before sending the key,
    // mirroring React's tabIndex=0 + onKeyDown on the section header.
    let (x, y) = rt.find("nav-tree-section-pinned").unwrap_or_else(|| panic!("section laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0);
    assert!(
        rt.state.events.iter().any(|a| matches!(a, RowAction::ToggleGroup(group) if group == PINNED_GROUP_KEY)),
        "Enter on the focused section toggles its group; events {:?}",
        rt.state.events
    );
}

#[test]
fn connection_actions_reach_the_host() {
    let row = row_vm(RowKind::Connection {
        id: "c1".into(), name: "Orders".into(), engine: "pg".into(),
        section_group: Some("team-a".into()), is_selected: false,
        status: RowConnectionStatus::Connected, expanded: true,
        match_reason: None, match_context: None, drop_before: false, drop_after: false,
    });
    let mut rt = Runtime::new(
        Probe { events: Vec::new(), row, dark: true },
        row_view,
        (1440.0, 816.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    let (x, y) = rt.find("conn-status-c1").unwrap_or_else(|| panic!("status dot laid out"));
    // The row owns the click; the status dot is decorative but keyed for lookup.
    let row_x = rt.find("nav-conn-c1").map(|p| p.0).unwrap_or(x);
    let _ = y;
    rt.pointer_down(row_x + 4.0, 12.0);
    rt.pointer_up(row_x + 4.0, 12.0);
    rt.frame(1.0);
    assert!(rt.state.events.iter().any(|a| matches!(a, RowAction::SelectConnection(id) if id == "c1")),
        "clicking a connection selects it; events {:?}", rt.state.events);
}

#[test]
fn rows_fit_both_windows_in_both_palettes() {
    let kinds = [
        RowKind::Section { section: "pinned".into(), display_name: "Pinned".into(), count: 1, expanded: true },
        RowKind::Group { group_name: "team-a".into(), display_name: "Team A".into(), count: 1, expanded: true, drop_target: false },
        RowKind::Connection {
            id: "c1".into(), name: "Long Connection Name".into(), engine: "postgresql".into(),
            section_group: None, is_selected: true, status: RowConnectionStatus::Connected,
            expanded: true, match_reason: None, match_context: None, drop_before: false, drop_after: false,
        },
        RowKind::Db { connection_id: "c1".into(), db_session_id: "s1".into(), db_name: "db1".into(), expanded: true, loading: false, is_open: true },
        RowKind::Schema { connection_id: "c1".into(), db_name: "db1".into(), schema_name: "public".into(), expanded: true },
        RowKind::Category { key: "c1::db1::public::tables".into(), cat_id: "tables".into(), label_key: "schemaTree.tables".into(), expanded: true },
        RowKind::Table { name: "users".into(), schema: Some("public".into()), cat_id: "tables".into(), is_selected: false, connection_id: "c1".into(), db_session_id: "s1".into(), db_name: "db1".into() },
        RowKind::Object { name: "fn_calc".into(), schema: None, cat_id: "routines".into() },
        RowKind::KvDb { connection_id: "c2".into(), db_session_id: "s2".into(), db_name: "db0".into(), is_selected: false, count: Some(3) },
        RowKind::DbLoading { owner_key: "conn:c2".into() },
        RowKind::DbError { message: "permission denied".into() },
        RowKind::NamespaceNode { name: "tbl".into(), expanded: true, is_leaf: false, key: "ns:db1::tbl".into(), connection_id: "c3".into(), db_session_id: "s3".into() },
        RowKind::EmptyGroup { group_name: Some("team-b".into()), drop_target: false },
        RowKind::NoConnections,
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        for dark in [true, false] {
            for size in [(960.0, 640.0), (1440.0, 816.0)] {
                let row = RowVm { kind: kind.clone(), depth: 2, level_depth: 2 };
                let mut rt = Runtime::new(
                    Probe { events: Vec::new(), row, dark },
                    row_view,
                    size,
                    Env { system_dark: dark, scale: 1.0 },
                );
                rt.frame(0.0);
                let outside: Vec<String> = rt.elements().iter()
                    .filter(|e| {
                        let r = e.rect;
                        r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                    })
                    .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                    .collect();
                assert!(outside.is_empty(), "variant {index} dark={dark} {size:?}: {outside:?}");
            }
        }
    }
}
