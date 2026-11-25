//! T495: `PrivilegeView` port — tree grouping, keys, actions, empty/loading
//! states and two-window/two-palette geometry.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::privilege_view::{
    build_object_tree, build_user_tree, privilege_view, GrantDialogVm, PrivilegeAction,
    PrivilegeGrantVm, PrivilegeHostPort, PrivilegeVm, ViewMode,
};

#[derive(Clone)]
struct Probe {
    vm: PrivilegeVm,
    grant: Option<GrantDialogVm>,
    options: Vec<String>,
    dark: bool,
    events: Vec<PrivilegeAction>,
}

fn grantee(user: &str, schema: Option<&str>, object: &str, privilege: &str) -> PrivilegeGrantVm {
    PrivilegeGrantVm {
        grantee: user.to_owned(),
        object_schema: schema.map(str::to_owned),
        object_name: object.to_owned(),
        privilege: privilege.to_owned(),
    }
}

fn probe() -> Probe {
    Probe {
        vm: PrivilegeVm {
            db_session_id: "s1".to_owned(),
            database: Some("app".to_owned()),
            grants: vec![
                grantee("alice", Some("public"), "users", "SELECT"),
                grantee("alice", Some("public"), "orders", "UPDATE"),
                grantee("bob", None, "", "SELECT"),
            ],
            supports_drop_user: true,
            // The host owns expansion; seed the default-open nodes React would show.
            expanded: ["user-alice", "user-bob", "db-current", "db-*", "schema-public"]
                .iter()
                .map(|key| (*key).to_owned())
                .collect(),
            ..PrivilegeVm::default()
        },
        grant: None,
        options: vec!["SELECT".to_owned(), "INSERT".to_owned()],
        dark: true,
        events: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let vm = s.vm.clone();
    let grant = s.grant.clone();
    let options = s.options.clone();
    let dark = s.dark;
    let host = PrivilegeHostPort {
        on_action: Some(Rc::new(move |st: &mut Probe, action| st.events.push(action))),
    };
    privilege_view(
        &vm,
        grant.as_ref(),
        &options,
        dark,
        &host,
        Some(Rc::new(|st: &mut Probe| &mut st.vm.sql)),
        Some(Rc::new(|st: &mut Probe| {
            &mut st.grant.get_or_insert_with(GrantDialogVm::default).username
        })),
        Some(Rc::new(|st: &mut Probe| {
            &mut st.grant.get_or_insert_with(GrantDialogVm::default).database
        })),
    )
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn tree_builders_group_by_user_and_by_object() {
    let grants = vec![
        grantee("alice", Some("public"), "users", "SELECT"),
        grantee("alice", Some("public"), "orders", "UPDATE"),
        grantee("bob", None, "", "SELECT"),
    ];
    let users = build_user_tree(&grants);
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].label, "alice");
    assert_eq!(users[0].count, Some(2), "per-user grant count");
    assert!(users[0].default_open, "<=5 users default open");
    let bob = &users[1];
    assert_eq!(bob.label, "bob");
    assert!(bob.children.iter().any(|db| db.label == "privileges.roleLevel"),
        "schema '*' + table '*' becomes a role-level node");

    let objects = build_object_tree(&grants);
    assert_eq!(objects.len(), 2, "current-db and role-level groups");
    let current = objects
        .iter()
        .find(|node| node.label == "privileges.currentDb")
        .expect("current-db object group");
    assert!(current.children.iter().any(|schema| schema.label == "public"));
}

#[test]
fn header_and_tree_keys_are_pinned() {
    let rt = runtime(probe(), (1440.0, 816.0));
    for key in [
        "privilege-view-root",
        "priv-header",
        "priv-mode-by-user",
        "priv-mode-by-object",
        "priv-grant-open",
        "priv-refresh",
        "privilege-view",
        "priv-tree-node-user-alice",
        "priv-tree-node-db-current",
        "priv-tree-node-schema-public",
        "priv-leaf-SELECT",
        "priv-sql-box",
        "priv-sql-input",
        "priv-sql-execute",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for key in [
        "privileges.title",
        "privileges.byUser",
        "privileges.byObject",
        "privileges.sqlHint",
        "query.execute",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated text");
    }
    assert!(rendered.iter().any(|text| text == "alice"), "grantee is shown");
    assert!(rendered.iter().any(|text| text == "users"), "object is shown");
}

#[test]
fn view_mode_refresh_and_toggle_dispatch() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    click(&mut rt, "priv-mode-by-object");
    assert_eq!(rt.state.events, [PrivilegeAction::SetViewMode(ViewMode::ByObject)]);

    let mut rt = runtime(probe(), (1440.0, 816.0));
    click(&mut rt, "priv-refresh");
    assert_eq!(rt.state.events, [PrivilegeAction::Load]);

    let mut rt = runtime(probe(), (1440.0, 816.0));
    click(&mut rt, "priv-tree-node-user-alice");
    assert!(rt.state.events.iter().any(|a| matches!(a, PrivilegeAction::ToggleNode(key) if key == "user-alice")));
}

#[test]
fn sql_execute_uses_the_panel_database_and_reports_result() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    click(&mut rt, "priv-sql-execute");
    assert!(rt.state.events.iter().any(|a| matches!(a,
        PrivilegeAction::ExecuteSql { sql, database }
            if sql.contains("GRANT SELECT") && database.as_deref() == Some("app")
    )), "the SQL is pinned to the panel database");
}

#[test]
fn loading_error_and_empty_states() {
    let mut p = probe();
    p.vm.loading = true;
    p.vm.grants.clear();
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "priv-loading"));
    assert!(!has(&rt, "privilege-view"), "tree is hidden while loading");

    let mut p = probe();
    p.vm.error = Some("denied".to_owned());
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "priv-error"));
    assert_eq!(element(&rt, "priv-error").text.as_deref(), Some("denied"));

    let mut p = probe();
    p.vm.grants.clear();
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "priv-empty"));
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(rendered.iter().any(|text| text == t("privileges.empty")));
}

#[test]
fn grant_dialog_actions_are_host_owned() {
    let mut p = probe();
    p.vm.show_grant = true;
    let mut dialog = GrantDialogVm::default();
    dialog.selected.insert("SELECT".to_owned());
    dialog.username = TextEdit::new("carol".to_owned());
    p.grant = Some(dialog);
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "priv-grant-dialog"));
    assert!(has(&rt, "priv-grant-user"));
    assert!(has(&rt, "priv-grant-option-SELECT"));
    click(&mut rt, "priv-grant-submit");
    assert!(rt.state.events.iter().any(|a| matches!(a, PrivilegeAction::Grant { username, privileges, .. }
        if username == "carol" && privileges.as_slice() == ["SELECT"]
    )), "the host receives the grant request");
}

#[test]
fn panel_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe();
            p.dark = dark;
            let rt = runtime(p, size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "dark={dark} {size:?}: {outside:?}");
        }
    }
}

// ── T495 independent audit (appended; reuses this module's Probe) ────────

fn expanded_keys(keys: &[&str]) -> std::collections::BTreeSet<String> {
    keys.iter().map(|k| (*k).to_owned()).collect()
}

/// React `ByObjectView` gives each privilege leaf a revoke button
/// (`PrivilegeView.tsx:718-721`, `:741-744`) carrying the table/schema as the
/// revoke object. The port's `privilege_tree` never gives a revoke target to
/// by-object nodes, so no revoke affordance is mounted at all.
#[test]
fn audit_by_object_view_offers_revoke() {
    let mut p = probe();
    p.vm.view_mode = ViewMode::ByObject;
    p.vm.grants = vec![grantee("alice", Some("public"), "users", "SELECT")];
    p.vm.expanded = expanded_keys(&["db-current", "schema-public", "table-users"]);
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "priv-revoke-SELECT"), "by-object leaves must offer a revoke button");
    click(&mut rt, "priv-revoke-SELECT");
    assert!(
        rt.state.events.iter().any(|a| matches!(a,
            PrivilegeAction::Revoke { username, object_name, privileges }
                if username == "alice" && object_name == "users" && privileges.as_slice() == ["SELECT"]
        )),
        "by-object revoke must pin the table as its object; events {:?}",
        rt.state.events
    );
}

/// React `ByUserView` table leaves revoke with `tableName` as the object
/// (`PrivilegeView.tsx:574`). The port stores `(privilege, None)` for table
/// leaves (`model.rs:143-146`), so the emitted object name is empty.
#[test]
fn audit_by_user_table_revoke_pins_the_table_name() {
    let mut p = probe();
    p.vm.grants = vec![grantee("alice", Some("public"), "users", "SELECT")];
    p.vm.expanded = expanded_keys(&["user-alice", "db-current", "schema-public"]);
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "priv-revoke-SELECT"));
    click(&mut rt, "priv-revoke-SELECT");
    assert!(
        rt.state.events.iter().any(|a| matches!(a,
            PrivilegeAction::Revoke { object_name, .. } if object_name == "users"
        )),
        "a table leaf's revoke must name the table; events {:?}",
        rt.state.events
    );
}

/// React's role-level leaf (`PrivilegeView.tsx:531-537`) passes no `onRevoke`,
/// so it renders no per-privilege X. The port gives every by-user leaf a
/// revoke target through `revoke_user`, including the role-level ones.
#[test]
fn audit_role_level_leaf_has_no_revoke() {
    let mut p = probe();
    p.vm.grants = vec![grantee("bob", None, "", "USAGE")];
    p.vm.expanded = expanded_keys(&["user-bob", "db-*", "role-level"]);
    let rt = runtime(p, (1440.0, 816.0));
    assert!(
        !has(&rt, "priv-revoke-USAGE"),
        "role-level grants must not offer revoke (React passes no onRevoke)"
    );
}

/// React's `ByObjectView` has no drop-user button (only `ByUserView` does,
/// gated by `supportsDropUser` at `PrivilegeView.tsx:500-514`). The port's
/// `tree_node` adds one to every `user-*` node regardless of view mode.
#[test]
fn audit_by_object_view_has_no_drop_user() {
    let mut p = probe();
    p.vm.view_mode = ViewMode::ByObject;
    p.vm.grants = vec![grantee("alice", Some("public"), "users", "SELECT")];
    p.vm.expanded = expanded_keys(&["db-current", "schema-public", "table-users"]);
    let rt = runtime(p, (1440.0, 816.0));
    assert!(
        !has(&rt, "priv-drop-user-alice"),
        "by-object user nodes must not offer drop-user"
    );
}
