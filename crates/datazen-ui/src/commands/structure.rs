//! Port of `packages/frontend/src/commands/structure.ts` (T397).
//!
//! The structure-editor command pair, both riding the target-command
//! rail (`executeTargetCommand` — one transport, the
//! [`TargetCommandRequest`] shaper shape T258 landed):
//!
//! - `getStructureCapabilities` — the driver's capability sheet for
//!   the session's current catalog position; `input` is the empty
//!   bag.
//! - `planTableStructureChanges` — the plan the backend's
//!   `plan_table_structure_changes` handler mints. The optional
//!   `database` is the plan's **target** (cross-database DDL resolves
//!   against the right catalog without switching the session), and
//!   the target's `schema` comes from the **request itself**
//!   (`request.schema ?? null`), not a separate argument — the
//!   React source's own doc comment records both decisions.
//!
//! The UI and backend share the SQL schema contract for the request.
//!
//! `metadataTarget`'s store-lookup half (resolve the connection by
//! `dbSessionId` when `connectionId` is absent) stays the host's
//! seam — this port takes the resolved owner, exactly as
//! `database.rs`'s [`metadata_target`] does; `structure.ts` always
//! passes the `connectionId` it was handed.

use serde_json::json;

pub use datazen_sql::schema::StructureChangeRequest;

use crate::commands::database::{metadata_target, DatabaseTarget, TargetCommandRequest};

/// `getStructureCapabilities`' target: the session's catalog position,
/// no object half (the React call passes `undefined` for the table).
pub fn capabilities_target(
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> DatabaseTarget {
    metadata_target(connection_id, database, schema, None)
}

/// `getStructureCapabilities(dbSessionId, database, schema,
/// connectionId)`.
pub fn get_structure_capabilities_request(target: &DatabaseTarget) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "get_structure_capabilities",
        input: json!({}),
    }
}

/// `planTableStructureChanges`' target: the plan's `database` is the
/// target and the `schema` is the request's own (`request.schema ??
/// null`) — a request without a schema plans against the default.
pub fn plan_target(
    connection_id: &str,
    database: Option<&str>,
    request: &StructureChangeRequest,
) -> DatabaseTarget {
    metadata_target(connection_id, database, request.schema.as_deref(), None)
}

/// `planTableStructureChanges(dbSessionId, request, database,
/// connectionId)`: the `{ request }` input bag, wire camelCase.
pub fn plan_table_structure_changes_request(
    target: &DatabaseTarget,
    request: &StructureChangeRequest,
) -> TargetCommandRequest {
    TargetCommandRequest {
        target: target.clone(),
        command: "plan_table_structure_changes",
        input: json!({ "request": request }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> StructureChangeRequest {
        serde_json::from_value(json!({ "mode": "create", "table": "users", "currentColumns": [] }))
            .expect("wire request decodes")
    }

    #[test]
    fn commands_ride_the_target_rail_with_verbatim_names() {
        let target = capabilities_target("conn-1", Some("app"), None);
        let shaped = get_structure_capabilities_request(&target);
        assert_eq!(shaped.command, "get_structure_capabilities");
        assert_eq!(shaped.input, json!({}));
        assert_eq!(shaped.target, target);

        let shaped = plan_table_structure_changes_request(&target, &request());
        assert_eq!(shaped.command, "plan_table_structure_changes");
        assert_eq!(shaped.target, target);
    }

    #[test]
    fn the_plan_input_bags_the_request_camel_case() {
        let target = plan_target("conn-1", None, &request());
        let shaped = plan_table_structure_changes_request(&target, &request());
        // The bag key and the wire fields the backend's
        // `StructurePlanInput` decode expects.
        assert_eq!(shaped.input["request"]["mode"], json!("create"));
        assert_eq!(shaped.input["request"]["table"], json!("users"));
        assert!(shaped.input["request"].get("schema").is_none());
        assert_eq!(shaped.input["request"]["currentColumns"], json!([]));
    }

    #[test]
    fn the_plan_target_takes_its_schema_from_the_request() {
        // A request with a schema plans against it…
        let with_schema: StructureChangeRequest = serde_json::from_value(json!({
            "mode": "create", "table": "users", "schema": "dbo", "currentColumns": []
        }))
        .expect("decodes");
        let target = plan_target("conn-1", Some("app"), &with_schema);
        assert_eq!(target.namespace.database.as_deref(), Some("app"));
        assert_eq!(target.namespace.schema.as_deref(), Some("dbo"));
        assert!(target.object.is_none());
        // …and one without plans against the default (`?? null`).
        let target = plan_target("conn-1", None, &request());
        assert_eq!(target.namespace.schema, None);
        assert_eq!(target.namespace.database, None);
    }

    #[test]
    fn the_capabilities_target_has_no_object_half() {
        let target = capabilities_target("conn-1", Some("app"), Some("dbo"));
        assert_eq!(target.connection_id, "conn-1");
        assert_eq!(target.namespace.database.as_deref(), Some("app"));
        assert_eq!(target.namespace.schema.as_deref(), Some("dbo"));
        assert!(target.object.is_none());
        // catalog and path stay the metadataTarget literals.
        assert_eq!(target.namespace.catalog, None);
        assert!(target.namespace.path.is_empty());
    }
}
