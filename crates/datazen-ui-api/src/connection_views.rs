//! Connection view contracts, ported from
//! `packages/frontend/src/lib/connectionViews/types.ts` — which itself only
//! re-exports `@datazen/driver-sdk`'s `types/connection-view.ts`, so this
//! module carries the Rust definitions of the four re-exported names.
//!
//! The vocabulary is what a driver-provided connection view receives from
//! the host workspace shell: the payload a node's context menu hands up, the
//! explicit connection a server-status/process-list panel binds to, the
//! table context the SQL actions carry, and the action surface the host
//! offers. Pure contracts — no Rune tree, no state — so the connection
//! window ports (tasklist T067/T110/T199) and the view registry port (T421)
//! consume them from here.
//!
//! React's `DatabaseType` is a string alias (`extension-points`'
//! `databaseTypes.ts`); the Rust side keeps the string. Deferred halves, with
//! their owners:
//!
//! - `onPanelClosed`, `selectTableRef`, `nodeContextMenuRef`, `actionsRef` —
//!   retained-mode plumbing (callback registration, `MutableRefObject`
//!   handshakes). Immediate-mode hosts answer per frame instead, so the
//!   wiring is designed by the workspace shell port (T110 ConnectionPage),
//!   not by this contract module.
//! - `kvSlotState` — the host-owned KV relay from driver-sdk `kv-slots.ts`
//!   and `lib/kvSlotState.ts`, ported by tasklist T545/T547.

/// The payload a connection view's context menu hands up for a node
/// (table, view, or blank canvas): what the node is and where the cursor was.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeContextMenuPayload {
    /// Driver-defined node kind (`"table"`, `"view"`, `"blank"`, …).
    pub kind: String,
    pub name: String,
    /// Cursor position in window coordinates, ready for menu placement.
    pub x: f32,
    pub y: f32,
    /// The node's schema, when the driver exposes schemas.
    pub schema: Option<String>,
}

/// The explicit connection a server panel binds to when the context menu
/// opens 服务器仪表盘 / 进程列表: the connection the user actually clicked,
/// not the ambient active one — so MySQL/PG panels cannot mix their data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionOpenTarget {
    /// Persistent connection id the user clicked.
    pub connection_id: String,
    /// Live database session id for that connection (may be resolved by the host).
    pub db_session_id: String,
    pub connection_name: String,
    /// Engine name (`DatabaseType` in React, a string alias).
    pub database_type: String,
}

/// The SQL action a table context menu asks the host to open:
/// React's `'openData' | 'select' | 'insert' | 'update' | 'ddl'` literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSqlActionKind {
    OpenData,
    Select,
    Insert,
    Update,
    Ddl,
}

impl TableSqlActionKind {
    /// The literal React (and its tests) knows this action by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenData => "openData",
            Self::Select => "select",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Ddl => "ddl",
        }
    }
}

/// The object kinds `openObject` opens: React's
/// `'function' | 'procedure' | 'trigger' | 'sequence' | 'type'` literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectKind {
    /// The literal React (and its tests) knows this object by.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }
}

/// The `Pick<TableContextInput, 'database' | 'schema'>` half `newQuery`
/// receives: which database (and schema inside it) the new editor targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewQueryContext {
    pub database: String,
    /// React's `string | null` — no schema concept on this engine.
    pub schema: Option<String>,
}

/// Everything a table-level action needs to know about the table it acts
/// on, as the navigator's menus build it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableContextInput {
    pub connection_id: String,
    pub db_session_id: String,
    /// Engine name (`DatabaseType` in React, a string alias).
    pub database_type: String,
    pub database: String,
    /// React's `string | null` — no schema concept on this engine.
    pub schema: Option<String>,
    pub table_name: Option<String>,
    /// React's `string | null`.
    pub table_schema: Option<String>,
    pub view_name: Option<String>,
    /// React's `string | null`.
    pub view_schema: Option<String>,
    pub object_type: Option<String>,
    pub name: Option<String>,
}

/// The data half of what a driver-provided connection view receives from the
/// host shell (`ConnectionViewProps` in React). The callback/`ref` halves are
/// deferred — see the module docs for their owners.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionViewProps {
    /// Identity of the tab this view is rendered into. **Opaque**: a key and
    /// nothing else — do not parse it, do not assume a format. Unique per tab
    /// and stable for the tab's whole life; unlike `db_session_id` two tabs on
    /// one connection never share it.
    pub panel_id: String,
    /// Live database session id used for every query this view issues.
    pub db_session_id: String,
    /// Persistent saved-connection id (stable across restarts).
    pub connection_id: String,
    pub connection_name: String,
    /// Engine name (`DatabaseType` in React, a string alias).
    pub database_type: String,
    pub initial_database: Option<String>,
    /// Hide the view's own schema sidebar (used with an outer navigator tree).
    pub hide_sidebar: Option<bool>,
    /// Whether this view is the currently active (visible) tab.
    pub is_active: Option<bool>,
}

/// The host-side action surface a connection view may invoke — the Rust form
/// of React's `ConnectionViewActions` callback bag.
///
/// React marks the optional actions with `?:` and every caller guards with
/// `actions.openSqlFile?.()`; here the same optionality is a default no-op
/// body, so calling an action the host did not wire up does nothing instead
/// of failing to compile or panicking. The three actions React requires a
/// host to provide (`newQuery`, `openErDiagram`, `refresh`) have no default
/// body — a host that cannot answer them does not implement the contract.
pub trait ConnectionViewActions {
    /// Open a new query editor; returns whether the host opened one.
    /// React's return is `boolean | void` — every existing caller ignores
    /// the value, and the `void` half has no Rust counterpart, so the
    /// implementations that answered nothing now answer `false`.
    fn new_query(
        &mut self,
        initial_sql: Option<&str>,
        context: Option<&NewQueryContext>,
        title: Option<&str>,
    ) -> bool;

    /// Open the ER diagram, optionally focusing one table in one database.
    fn open_er_diagram(&mut self, focus_table: Option<&str>, database: Option<&str>);

    /// Refresh the navigator tree this view belongs to.
    fn refresh(&mut self);

    /// Open a table through one of its SQL actions. Optional in React.
    fn open_table_action(&mut self, context: &TableContextInput, action: TableSqlActionKind) {
        let _ = (context, action);
    }

    /// Open a `.sql` file picker. Optional in React.
    fn open_sql_file(&mut self) {}

    /// Start creating a table. Optional in React.
    fn create_table(&mut self) {}

    /// Open the create-database dialog. Optional in React.
    fn open_create_database(&mut self) {}

    /// Open the create-schema dialog. Optional in React.
    fn open_create_schema(&mut self) {}

    /// Open the create-user dialog. Optional in React.
    fn open_create_user(&mut self) {}

    /// Open a table's structure. Optional in React: menus drop the item
    /// unless the host exposes it.
    fn open_table_structure(&mut self, table_name: &str) {
        let _ = table_name;
    }

    /// Open a database object's editor. Optional in React.
    fn open_object(
        &mut self,
        kind: ObjectKind,
        name: &str,
        schema: Option<&str>,
        signature: Option<&str>,
        target_schema: Option<&str>,
        target_name: Option<&str>,
        database: Option<&str>,
    ) {
        let _ = (kind, name, schema, signature, target_schema, target_name, database);
    }

    /// Open the query history. Optional in React.
    fn open_query_history(&mut self) {}

    /// Open the target connection's server dashboard; the context menu
    /// passes the clicked connection explicitly. Optional in React.
    fn open_server_status(&mut self, ctx: Option<&ConnectionOpenTarget>) {
        let _ = ctx;
    }

    /// Open the target connection's process list; the context menu passes
    /// the clicked connection explicitly. Optional in React.
    fn open_process_list(&mut self, ctx: Option<&ConnectionOpenTarget>) {
        let _ = ctx;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host that answers only what React requires every host to answer.
    #[derive(Default)]
    struct MinimalHost {
        log: Vec<String>,
        opens: bool,
    }

    impl ConnectionViewActions for MinimalHost {
        fn new_query(
            &mut self,
            initial_sql: Option<&str>,
            context: Option<&NewQueryContext>,
            title: Option<&str>,
        ) -> bool {
            self.log.push(format!(
                "newQuery({:?}, db={:?}, title={:?})",
                initial_sql,
                context.map(|c| c.database.as_str()),
                title
            ));
            self.opens
        }

        fn open_er_diagram(&mut self, focus_table: Option<&str>, database: Option<&str>) {
            self.log.push(format!("openErDiagram({:?}, {:?})", focus_table, database));
        }

        fn refresh(&mut self) {
            self.log.push("refresh".into());
        }
    }

    #[test]
    fn the_required_actions_reach_the_host() {
        let mut host = MinimalHost { log: Vec::new(), opens: true };
        let ctx = NewQueryContext { database: "shop".into(), schema: None };
        assert!(
            host.new_query(Some("select 1"), Some(&ctx), Some("scratch")),
            "the host's own answer comes back"
        );
        host.open_er_diagram(Some("orders"), Some("shop"));
        host.refresh();
        assert_eq!(
            host.log,
            vec![
                "newQuery(Some(\"select 1\"), db=Some(\"shop\"), title=Some(\"scratch\"))".to_owned(),
                "openErDiagram(Some(\"orders\"), Some(\"shop\"))".to_owned(),
                "refresh".to_owned(),
            ]
        );
    }

    #[test]
    fn an_unwired_optional_action_is_a_no_op_like_the_react_optional_chaining() {
        // React: `actions.openSqlFile?.()` on a host that provides nothing —
        // nothing happens. The default bodies here are the same nothing.
        let mut host = MinimalHost::default();
        host.open_sql_file();
        host.create_table();
        host.open_create_database();
        host.open_create_schema();
        host.open_create_user();
        host.open_query_history();
        host.open_table_structure("orders");
        let target = ConnectionOpenTarget {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            connection_name: "Prod".into(),
            database_type: "mysql".into(),
        };
        host.open_server_status(Some(&target));
        host.open_process_list(None);
        host.open_object(ObjectKind::Trigger, "t", None, None, None, None, None);
        let table = TableContextInput {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            database_type: "mysql".into(),
            database: "shop".into(),
            schema: None,
            table_name: Some("orders".into()),
            table_schema: None,
            view_name: None,
            view_schema: None,
            object_type: None,
            name: None,
        };
        host.open_table_action(&table, TableSqlActionKind::Insert);
        assert!(
            host.log.is_empty(),
            "the optional actions left the host untouched: {:?}",
            host.log
        );
        assert!(
            !host.new_query(None, None, None),
            "the required surface still answers with the host's own flag"
        );
        assert_eq!(host.log.len(), 1);
    }

    #[test]
    fn table_sql_action_kinds_keep_the_react_literals() {
        assert_eq!(TableSqlActionKind::OpenData.as_str(), "openData");
        assert_eq!(TableSqlActionKind::Select.as_str(), "select");
        assert_eq!(TableSqlActionKind::Insert.as_str(), "insert");
        assert_eq!(TableSqlActionKind::Update.as_str(), "update");
        assert_eq!(TableSqlActionKind::Ddl.as_str(), "ddl");
    }

    #[test]
    fn object_kinds_keep_the_react_literals() {
        assert_eq!(ObjectKind::Function.as_str(), "function");
        assert_eq!(ObjectKind::Procedure.as_str(), "procedure");
        assert_eq!(ObjectKind::Trigger.as_str(), "trigger");
        assert_eq!(ObjectKind::Sequence.as_str(), "sequence");
        assert_eq!(ObjectKind::Type.as_str(), "type");
    }

    #[test]
    fn optional_fields_map_onto_option_like_the_react_contract() {
        let payload = NodeContextMenuPayload {
            kind: "table".into(),
            name: "orders".into(),
            x: 612.0,
            y: 352.0,
            schema: Some("public".into()),
        };
        assert_eq!(payload.schema.as_deref(), Some("public"));
        let bare = NodeContextMenuPayload {
            schema: None,
            ..payload.clone()
        };
        assert_eq!(bare.schema, None);
        assert_eq!(bare.x, 612.0);

        let props = ConnectionViewProps {
            panel_id: "p-1".into(),
            db_session_id: "session-9".into(),
            connection_id: "c1".into(),
            connection_name: "Prod".into(),
            database_type: "postgres".into(),
            initial_database: Some("shop".into()),
            hide_sidebar: Some(true),
            is_active: None,
        };
        // Unset optionals stay unset rather than collapsing to a default.
        assert_eq!(props.is_active, None);
        assert_eq!(props.hide_sidebar, Some(true));
        assert_eq!(props.panel_id, "p-1");
    }
}
