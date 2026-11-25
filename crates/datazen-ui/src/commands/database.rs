//! Port of `packages/frontend/src/commands/database.ts` (T258, split) —
//! the database command family.
//!
//! The file's three rails, kept apart exactly as the React source keeps
//! them:
//!
//! 1. The **target-command rail** (`executeTargetCommand`,
//!    `lib/session/targetCommand.ts` — T311's, segment 1): the metadata
//!    reads ([`DatabaseTarget`] + command + input), with the discovery /
//!    catalog / objects / DDL / ER shapers. The active-session owner
//!    resolution (`metadataTarget`'s store lookup) is the host's seam —
//!    the host resolves the owner and builds the target; the store does
//!    not exist in this crate (the `ConnectionJoiner` ruling).
//! 2. The **domain-client rail** (the `useBackendClient()` pass-throughs,
//!    contract §6.x): the table read and its preview, the pending-change
//!    plan/commit, the row estimate, the foreign-key pair, the value
//!    search and the named views — one [`DatabaseMethod`] MethodMap
//!    entry each, the payloads verbatim.
//! 3. The **legacy rail**: `get_privileges`.
//!
//! `getDatabases`' in-flight de-duplication (`databaseDiscoveryInFlight`)
//! is [`DiscoveryJoiner`]: dedicated endpoint sessions share
//! connection-scoped discovery, session callers keep the session
//! boundary — the key is the JSON tuple `['session', id]` /
//! `['connection', id]`, and a settled promise frees its slot.
//!
//! The React file's result validations (`Array.isArray` guards) are the
//! [`validate_catalog`] / [`validate_objects`] / [`validate_ddl`]
//! decoders: a malformed answer is an error, never a half-empty catalog.
//!
//! Known deltas: none behavioural — every command name, payload key and
//! normalization (`?? null`, `?? undefined`) is the React source's
//! verbatim.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// The target rail (T311's vocabulary, projected)
// ---------------------------------------------------------------------------

/// React `ExecutionTarget` — the namespace the host resolves for a
/// metadata read (the engine projection; the store lookup is the host's
/// seam).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseTarget {
    pub connection_id: String,
    pub namespace: DatabaseNamespace,
    /// The object a table-scoped read targets (None for catalog reads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<DatabaseObjectRef>,
}

/// The namespace half (`database`/`schema`/`catalog`/`path`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseNamespace {
    pub database: Option<String>,
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

/// The object half (`{ kind, name, signature }`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseObjectRef {
    pub kind: String,
    pub name: String,
    pub signature: Option<String>,
}

/// One shaped target command (the `executeTargetCommand(target, { command,
/// input })` call, before execution).
#[derive(Clone, Debug, PartialEq)]
pub struct TargetCommandRequest {
    pub target: DatabaseTarget,
    pub command: &'static str,
    pub input: Value,
}

/// `metadataTarget(dbSessionId, database, schema, table?, connectionId?)`'s
/// engine half: the host supplies the resolved owner (the store lookup
/// stays the host's seam); the namespace carries the explicit halves.
pub fn metadata_target(
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: Option<&str>,
) -> DatabaseTarget {
    DatabaseTarget {
        connection_id: connection_id.to_owned(),
        namespace: DatabaseNamespace {
            database: database.map(str::to_owned),
            schema: schema.map(str::to_owned),
            catalog: None,
            path: Vec::new(),
        },
        object: table.map(|table| DatabaseObjectRef {
            kind: "relation".to_owned(),
            name: table.to_owned(),
            signature: None,
        }),
    }
}

/// `databaseDiscoveryTarget`: discovery belongs to the connection, before
/// any database is selected — the namespace is cleared whole.
pub fn database_discovery_target(connection_id: &str) -> DatabaseTarget {
    DatabaseTarget {
        connection_id: connection_id.to_owned(),
        namespace: DatabaseNamespace {
            database: None,
            schema: None,
            catalog: None,
            path: Vec::new(),
        },
        object: None,
    }
}

// ---------------------------------------------------------------------------
// The shapers (method for method)
// ---------------------------------------------------------------------------

/// `getDatabases`' wire half (the join decision is [`DiscoveryJoiner`]).
pub fn get_databases_request(target: &DatabaseTarget) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "list_databases",
        input: json!({}),
    }
}

/// `listCatalog`'s wire half; the catalog validation is
/// [`validate_catalog`].
pub fn list_catalog_request(
    target: &DatabaseTarget,
    database: &str,
    schema: Option<&str>,
) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "list_catalog",
        input: json!({ "database": database, "schema": schema }),
    }
}

/// `listTables`: the catalog's relations projected to the table-info
/// rows (`{ name, schema, tableType, rowCount }`), pure.
pub fn table_infos_from_catalog(catalog: &Value) -> Result<Vec<Value>, String> {
    let relations = catalog
        .get("relations")
        .and_then(Value::as_array)
        .ok_or_else(|| "Invalid catalog result".to_owned())?;
    Ok(relations
        .iter()
        .map(|relation| {
            json!({
                "name": relation.pointer("/ref/name").cloned().unwrap_or(Value::Null),
                "schema": relation.pointer("/ref/schema").cloned().unwrap_or(Value::Null),
                "tableType": relation.get("kind").cloned().unwrap_or(Value::Null),
                "rowCount": relation.get("rowCount").cloned().unwrap_or(Value::Null),
            })
        })
        .collect())
}

/// `listCatalog`'s `Array.isArray` guards: relations AND schemas must be
/// arrays, else `Invalid catalog result`.
pub fn validate_catalog(answer: &Value) -> Result<(), String> {
    let relations_ok = answer.get("relations").is_some_and(Value::is_array);
    let schemas_ok = answer.get("schemas").is_some_and(Value::is_array);
    if relations_ok && schemas_ok {
        Ok(())
    } else {
        Err("Invalid catalog result".to_owned())
    }
}

/// `getErData`'s per-relation schema request (the ER loop's body: the
/// target re-anchored to the relation, the exact identity carried).
pub fn er_table_schema_request(
    base: &DatabaseTarget,
    relation: &Value,
) -> Option<TargetCommandRequest> {
    let ref_name = relation.pointer("/ref/name")?.as_str()?.to_owned();
    let database = relation.pointer("/ref/database").and_then(Value::as_str).map(str::to_owned);
    let schema = relation.pointer("/ref/schema").and_then(Value::as_str).map(str::to_owned);
    let mut target = base.clone();
    target.namespace.database = database.clone();
    target.namespace.schema = schema.clone();
    target.object = Some(DatabaseObjectRef {
        kind: "relation".to_owned(),
        name: ref_name.clone(),
        signature: None,
    });
    Some(TargetCommandRequest {
        target,
        command: "get_table_schema",
        input: json!({
            "database": database,
            "schema": schema,
            "table": ref_name,
        }),
    })
}

/// `getErData`'s per-relation validation: a schema with a columns array,
/// else `Invalid schema result`.
pub fn validate_table_schema(answer: &Value) -> Result<(), String> {
    let schema = answer.get("schema").ok_or_else(|| "Invalid schema result".to_owned())?;
    if schema.get("columns").is_some_and(Value::is_array) {
        Ok(())
    } else {
        Err("Invalid schema result".to_owned())
    }
}

/// `getDatabaseObjects`' wire half; validation is [`validate_objects`].
pub fn list_objects_request(
    target: &DatabaseTarget,
    kind: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "list_objects",
        input: json!({ "kind": kind, "database": database, "schema": schema }),
    }
}

/// `getDatabaseObjects`' guard: `objects` must be an array.
pub fn validate_objects(answer: &Value) -> Result<(), String> {
    if answer.get("objects").is_some_and(Value::is_array) {
        Ok(())
    } else {
        Err("Invalid object catalog result".to_owned())
    }
}

/// `getObjectDdl`'s wire half; validation is [`validate_ddl`].
#[allow(clippy::too_many_arguments)]
pub fn get_object_ddl_request(
    target: &DatabaseTarget,
    kind: &str,
    name: &str,
    schema: Option<&str>,
    signature: Option<&str>,
    target_schema: Option<&str>,
    target_name: Option<&str>,
    database: Option<&str>,
) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "get_object_ddl",
        input: json!({
            "kind": kind,
            "name": name,
            "schema": schema,
            "signature": signature,
            "targetSchema": target_schema,
            "targetName": target_name,
            "database": database,
        }),
    }
}

/// `getObjectDdl`'s guard: `ddl` must be a string.
pub fn validate_ddl(answer: &Value) -> Result<(), String> {
    if answer.get("ddl").is_some_and(Value::is_string) {
        Ok(())
    } else {
        Err("Invalid object DDL result".to_owned())
    }
}

// ---------------------------------------------------------------------------
// The domain-client rail (the MethodMap)
// ---------------------------------------------------------------------------

/// The Database domain-client slice — one variant per `useBackendClient()`
/// pass-through; `wire_name` is the transport call name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseMethod {
    GetTableData,
    PreviewTableReadSql,
    PreviewPendingChanges,
    CommitPendingChanges,
    EstimateTableRows,
    ResolveForeignKeyTargets,
    ListInboundForeignKeys,
    SearchTableValues,
    ListNamedViews,
    SaveNamedView,
    DeleteNamedView,
}

impl DatabaseMethod {
    /// The domain client's method name.
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::GetTableData => "getTableData",
            Self::PreviewTableReadSql => "previewTableReadSql",
            Self::PreviewPendingChanges => "previewPendingChanges",
            Self::CommitPendingChanges => "commitPendingChanges",
            Self::EstimateTableRows => "estimateTableRows",
            Self::ResolveForeignKeyTargets => "resolveForeignKeyTargets",
            Self::ListInboundForeignKeys => "listInboundForeignKeys",
            Self::SearchTableValues => "searchTableValues",
            Self::ListNamedViews => "listNamedViews",
            Self::SaveNamedView => "saveNamedView",
            Self::DeleteNamedView => "deleteNamedView",
        }
    }
}

/// The table-read request's normalizations (`?? null` and the omitted
/// keys), applied to the caller's params bag.
pub fn normalize_table_read(params: &Value) -> Value {
    let mut out = json!({});
    if let Some(map) = params.as_object() {
        for (key, value) in map {
            out[key.clone()] = match key.as_str() {
                "manualHandle" | "rawWhere" => value.clone().into_or_null(),
                _ => value.clone(),
            };
        }
    }
    out
}

trait IntoOrNull {
    fn into_or_null(self) -> Value;
}

impl IntoOrNull for Value {
    fn into_or_null(self) -> Value {
        if self.is_null() {
            Value::Null
        } else {
            self
        }
    }
}

// ---------------------------------------------------------------------------
// The discovery joiner (`databaseDiscoveryInFlight`)
// ---------------------------------------------------------------------------

/// What [`DiscoveryJoiner::begin`] decided.
#[derive(Clone, Debug, PartialEq)]
pub enum DiscoveryDecision {
    /// The first caller for this key issues the request.
    Issue(TargetCommandRequest),
    /// An in-flight discovery already covers this key: join it.
    JoinInFlight,
}

/// `databaseDiscoveryInFlight`: one discovery per key, shared by every
/// concurrent caller. The key is the React tuple — `['session', id]`
/// for session-scoped discovery (no explicit owner), `['connection',
/// id]` for dedicated endpoints' connection-scoped discovery.
#[derive(Default)]
pub struct DiscoveryJoiner {
    in_flight: BTreeSet<String>,
}

impl DiscoveryJoiner {
    pub fn new() -> Self {
        Self::default()
    }

    /// The join key (`JSON.stringify(['session', id])` /
    /// `['connection', id]`).
    pub fn key(db_session_id: Option<&str>, connection_id: Option<&str>) -> String {
        match (db_session_id, connection_id) {
            (_, Some(connection)) => format!("[\"connection\",\"{connection}\"]"),
            (Some(session), None) => format!("[\"session\",\"{session}\"]"),
            (None, None) => "[\"session\",\"\"]".to_owned(),
        }
    }

    /// `getDatabases`' decision half: the first caller issues, concurrent
    /// callers join.
    pub fn begin(
        &mut self,
        db_session_id: Option<&str>,
        connection_id: Option<&str>,
    ) -> DiscoveryDecision {
        let key = Self::key(db_session_id, connection_id);
        if self.in_flight.insert(key) {
            // The discovery target: connection-owned when explicit.
            let target = match connection_id {
                Some(connection) => database_discovery_target(connection),
                None => {
                    // The session boundary: the host resolves the owner
                    // for the session (the store seam); the target's
                    // namespace is discovery-shaped either way.
                    database_discovery_target(db_session_id.unwrap_or_default())
                }
            };
            DiscoveryDecision::Issue(get_databases_request(&target))
        } else {
            DiscoveryDecision::JoinInFlight
        }
    }

    /// The settled promise frees its slot (`.finally`'s delete).
    pub fn finish(&mut self, db_session_id: Option<&str>, connection_id: Option<&str>) {
        self.in_flight.remove(&Self::key(db_session_id, connection_id));
    }

    /// Whether a discovery for this key is in flight.
    pub fn is_in_flight(&self, db_session_id: Option<&str>, connection_id: Option<&str>) -> bool {
        self.in_flight.contains(&Self::key(db_session_id, connection_id))
    }
}

// ---------------------------------------------------------------------------
// The legacy rail
// ---------------------------------------------------------------------------

/// One shaped legacy command.
#[derive(Clone, Debug, PartialEq)]
pub struct DatabaseLegacyRequest {
    pub command: &'static str,
    pub value: Value,
}

/// `getPrivileges(dbSessionId)` — the family's one legacy face.
pub fn get_privileges_request(db_session_id: &str) -> DatabaseLegacyRequest {
    DatabaseLegacyRequest {
        command: "get_privileges",
        value: json!({ "dbSessionId": db_session_id }),
    }
}
