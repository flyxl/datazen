//! Pure view models, actions and the host port for the query-builder adapter.
//!
//! Everything here is a 1:1 mirror of the React props / schema slice read by
//! `QueryBuilderHostAdapter.tsx` plus the `openPanelId` visibility slice owned
//! by `QueryEditorSection`. No host state, IPC or timer is created here.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::catalog::qb_context_key;

/// React `QueryBuilderPanelProps['onCommit']` mode.
#[allow(dead_code)] // action payload; consumer: the query-editor host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommitMode {
    Replace,
    Append,
}

/// One relation in the catalog: React's `{ name, schema }` shape.
#[allow(dead_code)] // catalog VM; consumer: the builder panel slot (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelationVm {
    pub name: String,
    pub schema: Option<String>,
}

/// React `TableSchema`-column metadata key held in `relationColumns`.
#[allow(dead_code)] // catalog VM; consumer: the catalog builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RelationColumnsVm {
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnVm>,
}

/// A column as seen by the builder's typed column map.
#[allow(dead_code)] // catalog VM; consumer: the catalog builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnVm {
    pub name: String,
    pub data_type: String,
}

/// The slice of `useSchemaStore` the adapter reads for one db session.
#[allow(dead_code)] // store projection; consumer: the query-editor host (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SessionSchemaVm {
    pub current_database: String,
    pub tables: Vec<RelationVm>,
    pub views: Vec<RelationVm>,
    pub table_catalogs: BTreeMap<String, Vec<RelationVm>>,
    pub path_items: BTreeMap<String, Vec<RelationVm>>,
    pub relation_columns: BTreeMap<String, RelationColumnsVm>,
}

/// React `QueryBuilderCatalog`: de-duplicated tables + the two column maps.
#[allow(dead_code)] // catalog VM; consumer: the builder panel slot (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryBuilderCatalogVm {
    pub tables: Vec<RelationVm>,
    pub column_map: BTreeMap<String, Vec<String>>,
    pub typed_column_map: BTreeMap<String, BTreeMap<String, String>>,
}

/// React `QueryBuilderHostAdapterProps` plus the contribution visibility slice
/// that decides whether the adapter is mounted at all.
#[allow(dead_code)] // view VM; consumer: the query-editor host (not wired).
#[derive(Clone, Debug)]
pub(crate) struct QueryBuilderVm {
    pub panel_id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub current_sql: String,
    /// Whether the host registered a `queryBuilder` extension contribution.
    pub contribution_available: bool,
    /// React `openPanelId` from `useQueryBuilderContribution()`.
    pub open_panel_id: Option<String>,
    /// The context key the contribution was last opened with (`openFor`).
    pub bound_context_key: Option<String>,
    pub session_schema: Option<SessionSchemaVm>,
    /// Host-resolved `DB_REGISTRY[type].sqlDialect`; `None` means fall back to
    /// the raw `database_type`, exactly like React's `?? databaseType`.
    pub dialect_override: Option<String>,
    pub enable_fk_prediction: bool,
    pub sql_format_options: String,
    /// React `query.visualBuilder.appliedToast` visibility.
    pub toast: Option<String>,
}

impl Default for QueryBuilderVm {
    fn default() -> Self {
        Self {
            panel_id: "query-panel-1".to_owned(),
            connection_id: String::new(),
            db_session_id: String::new(),
            database_type: String::new(),
            database: String::new(),
            schema: None,
            current_sql: String::new(),
            contribution_available: false,
            open_panel_id: None,
            bound_context_key: None,
            session_schema: None,
            dialect_override: None,
            enable_fk_prediction: false,
            sql_format_options: "standard".to_owned(),
            toast: None,
        }
    }
}

impl QueryBuilderVm {
    /// React `qbOpen = openPanelId === panelId`.
    #[allow(dead_code)] // pure rule; pinned by the adapter tests.
    pub(crate) fn is_open(&self) -> bool {
        self.contribution_available && self.open_panel_id.as_deref() == Some(self.panel_id.as_str())
    }

    /// React `JSON.stringify([connectionId ?? '', dbSessionId, database ?? '',
    /// schema ?? ''])` — the key passed to `openFor(panelId, contextKey)`.
    #[allow(dead_code)] // pure rule; pinned by the adapter tests.
    pub(crate) fn context_key(&self) -> String {
        qb_context_key(
            &self.connection_id,
            &self.db_session_id,
            &self.database,
            self.schema.as_deref(),
        )
    }

    /// The layout-effect rebind condition: open, but the contribution is bound
    /// to a stale database/schema context.
    #[allow(dead_code)] // pure rule; pinned by the adapter tests.
    pub(crate) fn needs_rebind(&self) -> bool {
        self.is_open() && self.bound_context_key.as_deref() != Some(self.context_key().as_str())
    }

    /// The rebind request the host must issue before paint when the context is
    /// stale; `None` when nothing needs rebinding.
    #[allow(dead_code)] // pure rule; pinned by the adapter tests.
    pub(crate) fn rebind_action(&self) -> Option<QueryBuilderAction> {
        self.needs_rebind().then_some(QueryBuilderAction::Rebind {
            panel_id: self.panel_id.clone(),
            context_key: self.context_key(),
        })
    }
}

/// Every command the adapter asks the host to perform. The service callbacks
/// the adapter hands to the Pro panel (`ensureColumns`, `loadTableSchema`,
/// `predictRelations`, `formatSql`) are requests here — no fake IPC or promise
/// lives in the view.
#[allow(dead_code)] // host port; consumer: the query-editor host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueryBuilderAction {
    /// Toolbar toggle: open for the current context, or hide if already open.
    Toggle {
        panel_id: String,
        context_key: String,
        open: bool,
    },
    /// Rebind an already-open builder to a changed database/schema context.
    Rebind {
        panel_id: String,
        context_key: String,
    },
    /// OK: write generated SQL to the editor and close with `'ok'`.
    Commit {
        sql: String,
        mode: CommitMode,
    },
    /// Cancel / ×: discard the draft and close with `'cancel'`.
    Cancel,
    EnsureColumns {
        db_session_id: String,
        database: String,
        schema: Option<String>,
        names: Vec<String>,
    },
    LoadTableSchema {
        name: String,
        schema: Option<String>,
    },
    PredictRelations {
        schemas: Vec<RelationVm>,
    },
    FormatSql {
        sql: String,
        database_type: String,
        options: String,
    },
}

#[allow(dead_code)] // host port; consumer: the query-editor host (not wired).
pub(crate) struct QueryBuilderHostPort<S> {
    pub on_action: Option<Rc<dyn Fn(&mut S, QueryBuilderAction) + 'static>>,
}

impl<S> Default for QueryBuilderHostPort<S> {
    fn default() -> Self {
        Self { on_action: None }
    }
}

impl<S: 'static> QueryBuilderHostPort<S> {
    #[allow(dead_code)] // host port; consumer: the query-editor host (not wired).
    pub(crate) fn emit(&self, state: &mut S, action: QueryBuilderAction) {
        if let Some(on_action) = &self.on_action {
            on_action(state, action);
        }
    }
}
