//! Port of `packages/frontend/src/commands/query.ts` (T356).
//!
//! The query faces, three families: the two executes ride T279's
//! driver gateway ([`crate::commands::driver`]) with `command:
//! 'query'` / `'query_stream'` and the params-when-non-empty input
//! rule; the user-asset writes (the history clear, the entry delete,
//! the favorite add/delete) ride the query-assets domain client with
//! the `saveUserAsset` submission token the host mints first (the
//! `ai::workflow` token face); the rest are plain reads or native
//! invokes, wire names verbatim.
//!
//! The `executeQuery`/`executeQueryStream` target rules ride along:
//! `database`/`schema` are the statement's target (the driver never
//! issues `USE` — it inlines the qualifier or routes to that
//! database's pool), and bound params ride the same cancellable
//! stream path.
//!
//! Known deltas: none behavioural — every command name and payload key
//! is the React source's verbatim; `HistorySort` defaults to
//! `'recent'` exactly as the wrapper does.

// No production consumer yet (the query view track wires the faces);
// this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::commands::ai::workflow::{AiSubmissionTokenVm, with_submission_token};
use crate::commands::driver::{
    DriverCommandRequest, ExecuteDriverCommand, ExecuteDriverCommandStream, execute_request,
    execute_stream_request,
};

/// `HistorySort`: the history page's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistorySort {
    #[default]
    Recent,
    Oldest,
}

/// The query-assets / backend-client faces the shapers ride (plus the
/// native invokes' command names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryMethod {
    CancelQuery,
    ClearQueryHistory,
    GetQueryHistoryPage,
    DeleteQueryHistory,
    SaveSqlFile,
    ListQueryFavorites,
    CreateQueryFavorite,
    DeleteQueryFavorite,
    GetFavoritesRoot,
    RefreshFavorites,
    BeginSessionTransaction,
    CommitSessionTransaction,
    RollbackSessionTransaction,
    SessionTransactionStatus,
    ResolveResultOrigin,
    PreviewResultCellChange,
}

impl QueryMethod {
    /// The wire name (the domain-client method or the invoke command).
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::CancelQuery => "cancel_query",
            Self::ClearQueryHistory => "clearQueryHistory",
            Self::GetQueryHistoryPage => "getQueryHistoryPage",
            Self::DeleteQueryHistory => "deleteQueryHistory",
            Self::SaveSqlFile => "save_sql_file",
            Self::ListQueryFavorites => "listQueryFavorites",
            Self::CreateQueryFavorite => "createQueryFavorite",
            Self::DeleteQueryFavorite => "deleteQueryFavorite",
            Self::GetFavoritesRoot => "get_favorites_root",
            Self::RefreshFavorites => "refresh_favorites",
            Self::BeginSessionTransaction => "begin_session_transaction",
            Self::CommitSessionTransaction => "commit_session_transaction",
            Self::RollbackSessionTransaction => "rollback_session_transaction",
            Self::SessionTransactionStatus => "session_transaction_status",
            Self::ResolveResultOrigin => "resolveResultOrigin",
            Self::PreviewResultCellChange => "previewResultCellChange",
        }
    }
}

/// The shaped call: the method plus its payload (the host runs it).
#[derive(Clone, Debug, PartialEq)]
pub struct QueryRequest {
    pub method: QueryMethod,
    pub value: Value,
}

impl QueryRequest {
    fn native(method: QueryMethod, value: Value) -> Self {
        Self { method, value }
    }
}

/// The params bag's shape (`Record<string, string | number | boolean
/// | null>`): a JSON object; an EMPTY object is dropped (the React
/// `Object.keys(params).length > 0` rule — `{ sql }` alone rides
/// when no param is bound).
fn query_input(sql: &str, params: Option<&Value>) -> Value {
    let has_params = params
        .and_then(Value::as_object)
        .is_some_and(|map| !map.is_empty());
    match (params, has_params) {
        (Some(params), true) => json!({ "sql": sql, "params": params }),
        _ => json!({ "sql": sql }),
    }
}

/// `executeQuery`: the driver execute with `command: 'query'`, the
/// statement's database/schema target, and the params-when-non-empty
/// input. The answer is the driver's `data` (the host casts).
pub fn execute_query_request(
    db_session_id: &str,
    sql: &str,
    params: Option<&Value>,
    database: Option<&str>,
    schema: Option<&str>,
) -> DriverCommandRequest {
    execute_request(&ExecuteDriverCommand {
        db_session_id: Some(db_session_id.to_owned()),
        driver_type: None,
        command: "query".to_owned(),
        input: query_input(sql, params),
        database: database.map(str::to_owned),
        schema: schema.map(str::to_owned),
    })
}

/// `executeQueryStream`'s caller options.
pub struct QueryStreamOptions<'a> {
    pub apply_result_limit: Option<bool>,
    pub record_history: Option<bool>,
    /// Database this stream reads from. Never a session `USE`.
    pub database: Option<&'a str>,
    /// PG-family schema target for the stream.
    pub schema: Option<&'a str>,
    /// Bound values ride the same cancellable stream path.
    pub params: Option<&'a Value>,
}

/// `executeQueryStream`: the driver stream execute with `command:
/// 'query_stream'` (the channel edge is the host's, T279's ruling).
pub fn execute_query_stream_request(
    db_session_id: &str,
    sql: &str,
    options: &QueryStreamOptions<'_>,
) -> DriverCommandRequest {
    execute_stream_request(&ExecuteDriverCommandStream {
        db_session_id,
        command: "query_stream",
        input: query_input(sql, options.params),
        database: options.database,
        schema: options.schema,
        apply_result_limit: options.apply_result_limit,
        record_history: options.record_history,
    })
}

/// `cancelQuery`.
pub fn cancel_query_request(db_session_id: &str, execution_id: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::CancelQuery,
        json!({ "dbSessionId": db_session_id, "executionId": execution_id }),
    )
}

/// `clearQueryHistory` (the token-carrying write): the host mints the
/// `saveUserAsset` token first and passes it here.
pub fn clear_query_history_request(token: &AiSubmissionTokenVm) -> QueryRequest {
    QueryRequest::native(QueryMethod::ClearQueryHistory, with_submission_token(json!({}), token))
}

/// `getQueryHistoryPage`'s predicates (every absent half reads null,
/// the order defaults to `recent` — the wrapper's own fill-ins).
#[derive(Clone, Debug, Default)]
pub struct QueryHistoryPageOptions {
    pub limit: u64,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub order: Option<HistorySort>,
}

/// `getQueryHistoryPage`: paged history with an honest count — every
/// predicate applies BEFORE the page is cut, `total` covers the whole
/// match set ("N of M", never "the page is everything"). The keys are
/// always present (null when unset — the React `?? null` fill-ins).
pub fn get_query_history_page_request(options: &QueryHistoryPageOptions) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::GetQueryHistoryPage,
        json!({
            "limit": options.limit,
            "connectionId": options.connection_id,
            "database": options.database,
            "schema": options.schema,
            "search": options.search,
            "since": options.since,
            "order": options.order.unwrap_or_default(),
        }),
    )
}

/// `deleteQueryHistoryEntry` — removes one row; resolves to 0 when the
/// id was already gone.
pub fn delete_query_history_entry_request(
    id: &str,
    token: &AiSubmissionTokenVm,
) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::DeleteQueryHistory,
        with_submission_token(json!({ "id": id }), token),
    )
}

/// `saveSqlFile`: ask for a save path, then write `content` to it
/// (resolves false when cancelled).
pub fn save_sql_file_request(default_file_name: &str, content: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::SaveSqlFile,
        json!({ "defaultFileName": default_file_name, "content": content }),
    )
}

/// `getFavoriteQueries`: the cached fast path.
pub fn get_favorite_queries_request(connection_id: Option<&str>) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::ListQueryFavorites,
        json!({ "connectionId": connection_id }),
    )
}

/// `addFavoriteQuery` (the token-carrying write).
pub fn add_favorite_query_request(
    connection_id: &str,
    title: &str,
    sql: &str,
    token: &AiSubmissionTokenVm,
) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::CreateQueryFavorite,
        with_submission_token(
            json!({ "connectionId": connection_id, "title": title, "sql": sql }),
            token,
        ),
    )
}

/// `deleteFavoriteQuery` (the token-carrying write).
pub fn delete_favorite_query_request(id: &str, token: &AiSubmissionTokenVm) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::DeleteQueryFavorite,
        with_submission_token(json!({ "id": id }), token),
    )
}

/// `getFavoritesRoot`: the resolved favorites directory, default or
/// configured (the panel shows it so the user knows which folder to
/// point a sync service at).
pub fn get_favorites_root_request() -> QueryRequest {
    QueryRequest::native(QueryMethod::GetFavoritesRoot, json!({}))
}

/// `refreshFavorites`: re-scan the directory, dropping the backend
/// cache first — the cached `getFavoriteQueries` will not see a `.sql`
/// file a sync client wrote while the app was running. The
/// `connectionId` key is ABSENT when unset (React passes the
/// `undefined` straight through and JSON drops it).
pub fn refresh_favorites_request(connection_id: Option<&str>) -> QueryRequest {
    let value = match connection_id {
        Some(connection_id) => json!({ "connectionId": connection_id }),
        None => json!({}),
    };
    QueryRequest::native(QueryMethod::RefreshFavorites, value)
}

/// `beginSessionTransaction`.
pub fn begin_session_transaction_request(db_session_id: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::BeginSessionTransaction,
        json!({ "dbSessionId": db_session_id }),
    )
}

/// `commitSessionTransaction`.
pub fn commit_session_transaction_request(db_session_id: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::CommitSessionTransaction,
        json!({ "dbSessionId": db_session_id }),
    )
}

/// `rollbackSessionTransaction`.
pub fn rollback_session_transaction_request(db_session_id: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::RollbackSessionTransaction,
        json!({ "dbSessionId": db_session_id }),
    )
}

/// `sessionTransactionStatus`.
pub fn session_transaction_status_request(db_session_id: &str) -> QueryRequest {
    QueryRequest::native(
        QueryMethod::SessionTransactionStatus,
        json!({ "dbSessionId": db_session_id }),
    )
}

/// `resolveResultOrigin`: ask where a result came from and whether it
/// may be written back — only the driver knows the rows' provenance,
/// so the interface renders the verdict and never recomputes it (a
/// failed call reads "read-only", never "writable"). The request is
/// the backend client's own shape; the port passes it through
/// verbatim.
pub fn resolve_result_origin_request(request: &Value) -> QueryRequest {
    QueryRequest::native(QueryMethod::ResolveResultOrigin, request.clone())
}

/// `previewResultCellChange`: the change plan for one edited cell —
/// the preview renders the statement and executes nothing; the commit
/// is a separate step, guarded by the plan's fingerprint.
pub fn preview_result_cell_change_request(request: &Value) -> QueryRequest {
    QueryRequest::native(QueryMethod::PreviewResultCellChange, request.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::ai::workflow::AiSubmissionTokenVm;

    fn token() -> AiSubmissionTokenVm {
        AiSubmissionTokenVm {
            idempotency_key: "idem-1".to_owned(),
            expires_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn the_executes_ride_the_driver_gateway() {
        // No params: the input is `{sql}` alone.
        let request = execute_query_request("s1", "SELECT 1", None, Some("app"), None);
        assert_eq!(request.command, "execute_driver_command");
        let wire = request.value;
        assert_eq!(wire["request"]["command"], "query");
        assert_eq!(wire["request"]["input"], json!({ "sql": "SELECT 1" }));
        assert_eq!(wire["request"]["dbSessionId"], "s1");
        assert_eq!(wire["request"]["database"], "app");
        assert!(wire["request"].get("schema").is_none(), "an unset schema is an absent key");

        // Params ride only when the bag is non-empty (an empty object
        // drops back to `{sql}`).
        let empty = execute_query_request("s1", "SELECT 1", Some(&json!({})), None, None);
        assert_eq!(empty.value["request"]["input"], json!({ "sql": "SELECT 1" }));
        let bound =
            execute_query_request("s1", "SELECT $1", Some(&json!({ "p": 1 })), None, Some("pub"));
        assert_eq!(
            bound.value["request"]["input"],
            json!({ "sql": "SELECT $1", "params": { "p": 1 } })
        );
        assert_eq!(bound.value["request"]["schema"], "pub");

        // The stream face: command 'query_stream', the options ride
        // beside the bag (null-normalized database/schema).
        let stream = execute_query_stream_request(
            "s1",
            "SELECT 1",
            &QueryStreamOptions {
                apply_result_limit: Some(true),
                record_history: None,
                database: None,
                schema: None,
                params: None,
            },
        );
        assert_eq!(stream.command, "execute_driver_command_stream");
        assert_eq!(stream.value["request"]["command"], "query_stream");
        assert_eq!(stream.value["request"]["input"], json!({ "sql": "SELECT 1" }));
        assert_eq!(stream.value["request"]["database"], Value::Null);
        assert_eq!(stream.value["applyResultLimit"], true);
        assert_eq!(stream.value["recordHistory"], Value::Null);
    }

    #[test]
    fn the_history_faces_shape_their_payloads() {
        let clear = clear_query_history_request(&token());
        assert_eq!(clear.method.wire_name(), "clearQueryHistory");
        assert!(clear.value.get("submissionToken").is_some() || clear.value.get("token").is_some(),
            "the minted token rides the request");

        let page = get_query_history_page_request(&QueryHistoryPageOptions {
            limit: 25,
            ..Default::default()
        });
        assert_eq!(page.method.wire_name(), "getQueryHistoryPage");
        assert_eq!(page.value["limit"], 25);
        assert_eq!(page.value["connectionId"], Value::Null, "the keys are always present");
        assert_eq!(page.value["order"], "recent", "the order default");
        let page = get_query_history_page_request(&QueryHistoryPageOptions {
            limit: 10,
            connection_id: Some("c1".to_owned()),
            search: Some("join".to_owned()),
            order: Some(HistorySort::Oldest),
            ..Default::default()
        });
        assert_eq!(page.value["connectionId"], "c1");
        assert_eq!(page.value["search"], "join");
        assert_eq!(page.value["order"], "oldest");

        let delete = delete_query_history_entry_request("h1", &token());
        assert_eq!(delete.method.wire_name(), "deleteQueryHistory");
        assert_eq!(delete.value["id"], "h1");
    }

    #[test]
    fn the_favorites_faces_shape_their_payloads() {
        let list = get_favorite_queries_request(Some("c1"));
        assert_eq!(list.method.wire_name(), "listQueryFavorites");
        assert_eq!(list.value["connectionId"], "c1");
        let list = get_favorite_queries_request(None);
        assert_eq!(list.value["connectionId"], Value::Null, "the React ?? null fill-in");

        let add = add_favorite_query_request("c1", "Title", "SELECT 1", &token());
        assert_eq!(add.method.wire_name(), "createQueryFavorite");
        assert_eq!(add.value["title"], "Title");

        let delete = delete_favorite_query_request("f1", &token());
        assert_eq!(delete.method.wire_name(), "deleteQueryFavorite");
        assert_eq!(delete.value["id"], "f1");

        assert_eq!(get_favorites_root_request().method.wire_name(), "get_favorites_root");

        // refreshFavorites: the connectionId key is ABSENT when unset.
        let refresh = refresh_favorites_request(Some("c1"));
        assert_eq!(refresh.method.wire_name(), "refresh_favorites");
        assert_eq!(refresh.value["connectionId"], "c1");
        let refresh = refresh_favorites_request(None);
        assert!(refresh.value.get("connectionId").is_none(), "undefined drops the key");
    }

    #[test]
    fn the_native_faces_shape_their_payloads() {
        let cancel = cancel_query_request("s1", "e1");
        assert_eq!(cancel.method.wire_name(), "cancel_query");
        assert_eq!(cancel.value["dbSessionId"], "s1");
        assert_eq!(cancel.value["executionId"], "e1");

        let save = save_sql_file_request("query.sql", "SELECT 1");
        assert_eq!(save.method.wire_name(), "save_sql_file");
        assert_eq!(save.value["defaultFileName"], "query.sql");
        assert_eq!(save.value["content"], "SELECT 1");

        for (request, name) in [
            (begin_session_transaction_request("s1"), "begin_session_transaction"),
            (commit_session_transaction_request("s1"), "commit_session_transaction"),
            (rollback_session_transaction_request("s1"), "rollback_session_transaction"),
            (session_transaction_status_request("s1"), "session_transaction_status"),
        ] {
            assert_eq!(request.method.wire_name(), name);
            assert_eq!(request.value["dbSessionId"], "s1");
        }

        // The pass-throughs ride verbatim.
        let origin = resolve_result_origin_request(&json!({ "resultId": "r1" }));
        assert_eq!(origin.method.wire_name(), "resolveResultOrigin");
        assert_eq!(origin.value["resultId"], "r1");
        let preview = preview_result_cell_change_request(&json!({ "cell": 1 }));
        assert_eq!(preview.method.wire_name(), "previewResultCellChange");
        assert_eq!(preview.value["cell"], 1);
    }
}
