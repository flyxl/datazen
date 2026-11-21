use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowValidationConnection {
    pub id: String,
    pub requires_explicit_database: bool,
}

fn text_of(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_owned()
}

fn is_record(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Object(_)))
}

fn has_entries(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Array(items)) if !items.is_empty())
}

struct Scope {
    label: String,
    steps: Vec<Value>,
    connection: String,
    database: String,
}

fn find_step_missing_database(
    obj: &Value,
    connections: Option<&[WorkflowValidationConnection]>,
) -> Option<String> {
    let connections = connections?;
    if connections.is_empty() {
        return None;
    }
    let by_id: HashMap<&str, &WorkflowValidationConnection> = connections
        .iter()
        .map(|connection| (connection.id.as_str(), connection))
        .collect();
    let object = obj.as_object()?;
    let workflow_database = text_of(object.get("database"));
    let workflow_connection = text_of(object.get("connection"));
    let blocks: Vec<Value> = match object.get("blocks") {
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };

    let mut scopes = Vec::new();
    if let Some(Value::Array(steps)) = object.get("steps") {
        scopes.push(Scope {
            label: "steps".to_owned(),
            steps: steps.clone(),
            connection: workflow_connection.clone(),
            database: workflow_database.clone(),
        });
    }
    for (index, block) in blocks.iter().enumerate() {
        if !is_record(Some(block)) {
            continue;
        }
        let block_object = match block.as_object() {
            Some(object) => object,
            None => continue,
        };
        let steps = match block_object.get("steps") {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        let connection = {
            let value = text_of(block_object.get("connection"));
            if value.is_empty() {
                workflow_connection.clone()
            } else {
                value
            }
        };
        let database = {
            let value = text_of(block_object.get("database"));
            if value.is_empty() {
                workflow_database.clone()
            } else {
                value
            }
        };
        scopes.push(Scope {
            label: format!("blocks[{index}].steps"),
            steps,
            connection,
            database,
        });
    }

    for scope in scopes {
        for (index, step) in scope.steps.iter().enumerate() {
            if !is_record(Some(step)) {
                continue;
            }
            let step_object = match step.as_object() {
                Some(object) => object,
                None => continue,
            };
            let step_type = text_of(step_object.get("type"));
            if step_type != "query" && step_type != "command" {
                continue;
            }
            let connection_id = {
                let value = text_of(step_object.get("connection"));
                if value.is_empty() {
                    scope.connection.clone()
                } else {
                    value
                }
            };
            let connection = match by_id.get(connection_id.as_str()) {
                Some(connection) => *connection,
                None => continue,
            };
            if !connection.requires_explicit_database {
                continue;
            }
            let step_database = if step_type == "query" {
                text_of(step_object.get("database"))
            } else {
                text_of(
                    step_object
                        .get("input")
                        .and_then(|input| input.get("database")),
                )
            };
            if !step_database.is_empty() || !scope.database.is_empty() {
                continue;
            }
            return Some(format!("{}[{index}].database", scope.label));
        }
    }
    None
}

pub fn validate_workflow_fields(
    obj: &Value,
    connections: Option<&[WorkflowValidationConnection]>,
) -> Option<String> {
    let object = obj.as_object()?;
    let id = object.get("id").and_then(Value::as_str);
    if id.map_or(true, str::is_empty) {
        return Some("id".to_owned());
    }
    let name = object.get("name").and_then(Value::as_str);
    if name.map_or(true, str::is_empty) {
        return Some("name".to_owned());
    }
    if !has_entries(object.get("steps")) && !has_entries(object.get("blocks")) {
        return Some("steps".to_owned());
    }
    find_step_missing_database(obj, connections)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn connection(id: &str, requires: bool) -> WorkflowValidationConnection {
        WorkflowValidationConnection {
            id: id.to_owned(),
            requires_explicit_database: requires,
        }
    }

    #[test]
    fn shape_checks_report_the_first_missing_field() {
        assert_eq!(
            validate_workflow_fields(&json!({}), None).as_deref(),
            Some("id")
        );
        assert_eq!(
            validate_workflow_fields(&json!({"id": 3}), None).as_deref(),
            Some("id")
        );
        assert_eq!(
            validate_workflow_fields(&json!({"id": "w"}), None).as_deref(),
            Some("name")
        );
        assert_eq!(
            validate_workflow_fields(&json!({"id": "w", "name": "n"}), None).as_deref(),
            Some("steps")
        );
        assert_eq!(
            validate_workflow_fields(
                &json!({"id": "w", "name": "n", "blocks": [{"id": "b"}]}),
                None
            ),
            None
        );
    }

    #[test]
    fn database_rule_uses_step_then_scope_then_workflow_chain() {
        let connections = [connection("multi", true), connection("single", false)];
        let obj = json!({
            "id": "w", "name": "n", "connection": "multi",
            "steps": [
                {"id": "s1", "type": "query", "sql": "select 1"},
                {"id": "s2", "type": "query", "sql": "select 1", "database": "shop"}
            ]
        });
        assert_eq!(
            validate_workflow_fields(&obj, Some(&connections)).as_deref(),
            Some("steps[0].database")
        );

        let with_workflow_default = json!({
            "id": "w", "name": "n", "connection": "multi", "database": "shop",
            "steps": [{"id": "s1", "type": "query", "sql": "select 1"}]
        });
        assert_eq!(
            validate_workflow_fields(&with_workflow_default, Some(&connections)),
            None
        );
    }

    #[test]
    fn command_steps_read_database_from_input() {
        let connections = [connection("multi", true)];
        let obj = json!({
            "id": "w", "name": "n", "steps": [
                {"id": "s1", "type": "command", "connection": "multi", "input": {"database": ""}}
            ]
        });
        assert_eq!(
            validate_workflow_fields(&obj, Some(&connections)).as_deref(),
            Some("steps[0].database")
        );
        let with_database = json!({
            "id": "w", "name": "n", "steps": [
                {"id": "s1", "type": "command", "connection": "multi", "input": {"database": "shop"}}
            ]
        });
        assert_eq!(
            validate_workflow_fields(&with_database, Some(&connections)),
            None
        );
    }

    #[test]
    fn block_scopes_use_block_targets_before_workflow_defaults() {
        let connections = [connection("multi", true)];
        let obj = json!({
            "id": "w", "name": "n", "connection": "single",
            "blocks": [
                {"id": "b", "connection": "multi", "steps": [
                    {"id": "s", "type": "query", "sql": "select 1"}
                ]},
                {"id": "b2", "connection": "multi", "database": "audit", "steps": [
                    {"id": "s", "type": "query", "sql": "select 1"}
                ]}
            ]
        });
        assert_eq!(
            validate_workflow_fields(&obj, Some(&connections)).as_deref(),
            Some("blocks[0].steps[0].database")
        );
    }

    #[test]
    fn non_data_steps_and_unknown_connections_are_skipped() {
        let connections = [connection("multi", true)];
        let obj = json!({
            "id": "w", "name": "n",
            "steps": [
                {"id": "s1", "type": "ai", "connection": "multi"},
                {"id": "s2", "type": "query", "connection": "unknown"}
            ]
        });
        assert_eq!(validate_workflow_fields(&obj, Some(&connections)), None);
        assert_eq!(validate_workflow_fields(&obj, Some(&[])), None);
    }
}
