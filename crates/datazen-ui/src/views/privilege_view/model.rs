//! Pure models, tree builders and host port for the privilege view.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use rune::TextEdit;

/// React `PrivilegeGrant`.
#[allow(dead_code)] // view VM; consumer: the privilege host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivilegeGrantVm {
    pub grantee: String,
    pub object_schema: Option<String>,
    pub object_name: String,
    pub privilege: String,
}

/// React `ViewMode`.
#[allow(dead_code)] // view VM; consumer: the privilege host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    #[default]
    ByUser,
    ByObject,
}

/// A tree node used by both views: a label at a depth with optional count and
/// an optional list of privilege leaves.
#[allow(dead_code)] // render VM; consumer: the tree renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrivilegeNode {
    pub key: String,
    pub label: String,
    pub depth: usize,
    pub count: Option<usize>,
    /// Leaves: (privilege, optional revoke target database/object).
    pub leaves: Vec<(String, Option<String>)>,
    pub children: Vec<PrivilegeNode>,
    /// Default-open rule from React (`users.length <= 5`, `userEntries <= 3`).
    pub default_open: bool,
}

/// React `buildUserTree` + the sorted user list, folded into render nodes.
#[allow(dead_code)] // pure rule; pinned by the privilege tests.
pub(crate) fn build_user_tree(grants: &[PrivilegeGrantVm]) -> Vec<PrivilegeNode> {
    // user → dbKey → schema → table → privileges
    let mut tree: BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>> =
        BTreeMap::new();
    for grant in grants {
        let schema = grant.object_schema.clone().unwrap_or_else(|| "*".to_owned());
        let table = if grant.object_name.is_empty() {
            "*".to_owned()
        } else {
            grant.object_name.clone()
        };
        let db_key = if schema == "*" && table == "*" { "*" } else { "current" };
        let entry = tree
            .entry(grant.grantee.clone())
            .or_default()
            .entry(db_key.to_owned())
            .or_default()
            .entry(schema)
            .or_default()
            .entry(table)
            .or_default();
        if !entry.contains(&grant.privilege) {
            entry.push(grant.privilege.clone());
        }
    }
    let default_open = tree.len() <= 5;
    tree.into_iter()
        .map(|(user, databases)| {
            let count = grants.iter().filter(|g| g.grantee == user).count();
            let children = databases
                .into_iter()
                .map(|(db_key, schemas)| {
                    let db_label = if db_key == "*" { "privileges.roleLevel" } else { "privileges.currentDb" };
                    let schema_children = schemas
                        .into_iter()
                        .map(|(schema, tables)| schema_node(&schema, tables, 2))
                        .collect();
                    PrivilegeNode {
                        key: format!("db-{db_key}"),
                        label: db_label.to_owned(),
                        depth: 1,
                        count: None,
                        leaves: Vec::new(),
                        children: schema_children,
                        default_open: true,
                    }
                })
                .collect();
            PrivilegeNode {
                key: format!("user-{user}"),
                label: user,
                depth: 0,
                count: Some(count),
                leaves: Vec::new(),
                children,
                default_open,
            }
        })
        .collect()
}

fn schema_node(
    schema: &str,
    tables: BTreeMap<String, Vec<String>>,
    depth: usize,
) -> PrivilegeNode {
    if schema == "*" {
        let all: Vec<String> = tables.into_values().flatten().collect();
        return PrivilegeNode {
            key: "role-level".to_owned(),
            label: "privileges.roleLevel".to_owned(),
            depth,
            count: None,
            leaves: all.into_iter().map(|p| (p, None)).collect(),
            children: Vec::new(),
            default_open: false,
        };
    }
    if tables.len() == 1 && tables.contains_key("*") {
        let privileges = tables.get("*").cloned().unwrap_or_default();
        return PrivilegeNode {
            key: format!("schema-{schema}"),
            label: schema.to_owned(),
            depth,
            count: None,
            leaves: privileges.into_iter().map(|p| (p, Some(schema.to_owned()))).collect(),
            children: Vec::new(),
            default_open: true,
        };
    }
    let count = tables.len();
    let children: Vec<PrivilegeNode> = tables
        .into_iter()
        .map(|(table, privileges)| PrivilegeNode {
            key: format!("table-{table}"),
            label: if table == "*" { "privileges.allTables".to_owned() } else { table },
            depth: depth + 1,
            count: None,
            leaves: privileges.into_iter().map(|p| (p, None)).collect(),
            children: Vec::new(),
            default_open: false,
        })
        .collect();
    PrivilegeNode {
        key: format!("schema-{schema}"),
        label: schema.to_owned(),
        depth,
        count: Some(count),
        leaves: Vec::new(),
        children,
        default_open: true,
    }
}

/// React `buildObjectTree`, folded into render nodes.
#[allow(dead_code)] // pure rule; pinned by the privilege tests.
pub(crate) fn build_object_tree(grants: &[PrivilegeGrantVm]) -> Vec<PrivilegeNode> {
    // dbKey → schema → table → user → privileges
    let mut tree: BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<String>>>>> =
        BTreeMap::new();
    for grant in grants {
        let schema = grant.object_schema.clone().unwrap_or_else(|| "*".to_owned());
        let table = if grant.object_name.is_empty() {
            "*".to_owned()
        } else {
            grant.object_name.clone()
        };
        let db_key = if schema == "*" && table == "*" { "*" } else { "current" };
        let entry = tree
            .entry(db_key.to_owned())
            .or_default()
            .entry(schema)
            .or_default()
            .entry(table)
            .or_default()
            .entry(grant.grantee.clone())
            .or_default();
        if !entry.contains(&grant.privilege) {
            entry.push(grant.privilege.clone());
        }
    }
    tree.into_iter()
        .map(|(db_key, schemas)| {
            let db_label = if db_key == "*" { "privileges.roleLevel" } else { "privileges.currentDb" };
            let schema_children = schemas
                .into_iter()
                .map(|(schema, tables)| {
                    let children: Vec<PrivilegeNode> = tables
                        .into_iter()
                        .map(|(table, users)| {
                            let user_children: Vec<PrivilegeNode> = users
                                .into_iter()
                                .map(|(user, privileges)| PrivilegeNode {
                                    key: format!("user-{user}"),
                                    label: user,
                                    depth: 3,
                                    count: None,
                                    leaves: privileges.into_iter().map(|p| (p, None)).collect(),
                                    children: Vec::new(),
                                    default_open: false,
                                })
                                .collect();
                            let default_open = user_children.len() <= 3;
                            PrivilegeNode {
                                key: format!("table-{table}"),
                                label: table,
                                depth: 2,
                                count: Some(user_children.len()),
                                leaves: Vec::new(),
                                children: user_children,
                                default_open,
                            }
                        })
                        .collect();
                    PrivilegeNode {
                        key: format!("schema-{schema}"),
                        label: schema,
                        depth: 1,
                        count: Some(children.len()),
                        leaves: Vec::new(),
                        children,
                        default_open: true,
                    }
                })
                .collect();
            PrivilegeNode {
                key: format!("db-{db_key}"),
                label: db_label.to_owned(),
                depth: 0,
                count: None,
                leaves: Vec::new(),
                children: schema_children,
                default_open: true,
            }
        })
        .collect()
}

/// Every host command the privilege view can request.
#[allow(dead_code)] // host port; consumer: the content-view host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PrivilegeAction {
    Load,
    SetViewMode(ViewMode),
    ToggleNode(String),
    ExecuteSql { sql: String, database: Option<String> },
    Grant { username: String, database: String, privileges: Vec<String> },
    Revoke { username: String, object_name: String, privileges: Vec<String> },
    DropUser(String),
    DismissError,
    SelectAllPrivileges,
    DeselectAllPrivileges,
    ToggleGrantPrivilege(String),
    OpenGrantDialog(Option<String>),
    CloseGrantDialog,
}

#[allow(dead_code)] // host port; consumer: the content-view host (not wired).
pub(crate) struct PrivilegeHostPort<S> {
    pub on_action: Option<Rc<dyn Fn(&mut S, PrivilegeAction) + 'static>>,
}

impl<S> Default for PrivilegeHostPort<S> {
    fn default() -> Self {
        Self { on_action: None }
    }
}

impl<S: 'static> PrivilegeHostPort<S> {
    pub(crate) fn emit(&self, state: &mut S, action: PrivilegeAction) {
        if let Some(on_action) = &self.on_action {
            on_action(state, action);
        }
    }
}

/// The view's state slice (React's `useState` values).
#[allow(dead_code)] // view VM; consumer: the content-view host (not wired).
#[derive(Clone, Debug)]
pub(crate) struct PrivilegeVm {
    pub db_session_id: String,
    pub database: Option<String>,
    pub grants: Vec<PrivilegeGrantVm>,
    pub loading: bool,
    pub error: Option<String>,
    pub view_mode: ViewMode,
    pub sql: TextEdit,
    pub running: bool,
    pub run_message: Option<String>,
    pub action_error: Option<String>,
    pub show_grant: bool,
    pub grant_user: Option<String>,
    /// Whether the connection declares the `drop_user` command.
    pub supports_drop_user: bool,
    pub expanded: BTreeSet<String>,
}

impl Default for PrivilegeVm {
    fn default() -> Self {
        Self {
            db_session_id: String::new(),
            database: None,
            grants: Vec::new(),
            loading: false,
            error: None,
            view_mode: ViewMode::ByUser,
            sql: TextEdit::new("GRANT SELECT ON TABLE schema.table TO role;".to_owned()),
            running: false,
            run_message: None,
            action_error: None,
            show_grant: false,
            grant_user: None,
            supports_drop_user: false,
            expanded: BTreeSet::new(),
        }
    }
}

impl PrivilegeVm {
    /// React `uniqueGrantees` — sorted distinct grantees for the grant dialog.
    #[allow(dead_code)] // pure rule; pinned by the privilege tests.
    pub(crate) fn unique_grantees(&self) -> Vec<String> {
        let mut users: Vec<String> = self.grants.iter().map(|g| g.grantee.clone()).collect();
        users.sort();
        users.dedup();
        users
    }
}
