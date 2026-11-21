use serde_json::{Map, Value};

use crate::{
    ErrorHandlingConfig, WorkflowBlock, WorkflowBlockRetry, WorkflowBlockType, WorkflowDefinition,
    WorkflowStep, WorkflowVariable, WorkflowVisibility,
};

use super::validate::validate_workflow_fields;

const STEP_ERROR: &str = "Invalid workflow field: steps must be a non-empty array of step objects";

fn is_record(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Object(_)))
}

fn text_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn is_workflow_variable(value: &Value) -> bool {
    match value.as_object() {
        Some(object) => {
            object.get("name").and_then(Value::as_str).is_some()
                && object.get("varType").and_then(Value::as_str).is_some()
        }
        None => false,
    }
}

fn is_workflow_step(value: &Value) -> bool {
    match value.as_object() {
        Some(object) => {
            object.get("id").and_then(Value::as_str).is_some()
                && object.get("type").and_then(Value::as_str).is_some()
        }
        None => false,
    }
}

fn to_step(value: &Value) -> Result<WorkflowStep, String> {
    serde_json::from_value(value.clone()).map_err(|_| STEP_ERROR.to_owned())
}

fn to_variable(value: &Value) -> WorkflowVariable {
    let object = value.as_object();
    let field = |key: &str| object.and_then(|object| text_field(object, key));
    WorkflowVariable {
        name: field("name").unwrap_or_default(),
        r#type: field("varType")
            .or_else(|| field("type"))
            .unwrap_or_default(),
        description: field("description").unwrap_or_default(),
        required: object
            .and_then(|object| object.get("required"))
            .and_then(Value::as_bool),
        default: object.and_then(|object| object.get("default")).cloned(),
    }
}

pub fn parse_workflow_yaml<P>(yaml: &str, parse: P) -> Result<Value, String>
where
    P: Fn(&str) -> Result<Value, String>,
{
    let parsed = parse(yaml)?;
    if !is_record(Some(&parsed)) {
        return Err("YAML must be an object".to_owned());
    }
    Ok(parsed)
}

pub fn parse_workflow_blocks(value: Option<&Value>) -> Result<Vec<WorkflowBlock>, String> {
    let items = match value {
        None => return Ok(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            return Err(
                "Invalid workflow field: blocks must be an array of block objects".to_owned(),
            )
        }
    };
    let mut seen = std::collections::HashSet::new();
    let mut blocks = Vec::with_capacity(items.len());
    for (index, raw) in items.iter().enumerate() {
        let object = match raw.as_object() {
            Some(object) => object,
            None => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}] must be an object"
                ))
            }
        };
        let id = match text_field(object, "id") {
            Some(id) if !id.trim().is_empty() => id,
            _ => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}].id must be a non-empty string"
                ))
            }
        };
        if !seen.insert(id.clone()) {
            return Err(format!(
                "Invalid workflow field: blocks[{index}].id duplicates block '{id}'"
            ));
        }
        let block_type = match text_field(object, "type").as_deref() {
            Some("sessionBlock") => WorkflowBlockType::SessionBlock,
            Some("transactionBlock") => WorkflowBlockType::TransactionBlock,
            _ => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}].type must be sessionBlock or transactionBlock"
                ))
            }
        };
        let steps = match object.get("steps") {
            Some(Value::Array(items)) if !items.is_empty() && items.iter().all(is_workflow_step) => {
                items.iter().map(to_step).collect::<Result<Vec<_>, _>>()?
            }
            _ => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}].steps must be a non-empty array of step objects"
                ))
            }
        };
        let parallel = match object.get("parallel") {
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}].parallel must be a boolean"
                ))
            }
            None => None,
        };
        let retry = match object.get("retry") {
            None | Some(Value::Null) => None,
            Some(Value::Object(retry)) => match retry.get("block") {
                Some(Value::Number(number))
                    if number.as_u64().is_some()
                        && number.as_u64().is_some_and(|value| value <= u32::MAX as u64) =>
                {
                    Some(WorkflowBlockRetry {
                        block: number.as_u64().unwrap_or(0) as u32,
                    })
                }
                _ => {
                    return Err(format!(
                        "Invalid workflow field: blocks[{index}].retry must be {{ block: <non-negative integer> }}"
                    ))
                }
            },
            Some(_) => {
                return Err(format!(
                    "Invalid workflow field: blocks[{index}].retry must be {{ block: <non-negative integer> }}"
                ))
            }
        };
        blocks.push(WorkflowBlock {
            id,
            r#type: block_type,
            connection: text_field(object, "connection"),
            database: text_field(object, "database"),
            parallel,
            steps,
            retry,
        });
    }
    Ok(blocks)
}

pub fn parse_validated_workflow_definition(obj: &Value) -> Result<WorkflowDefinition, String> {
    if let Some(missing) = validate_workflow_fields(obj, None) {
        return Err(format!("Missing required workflow field: {missing}"));
    }
    let object = match obj.as_object() {
        Some(object) => object,
        None => return Err("Missing required workflow field: id".to_owned()),
    };

    if let Some(description) = object.get("description") {
        if description.as_str().is_none() {
            return Err("Invalid workflow field: description must be a string".to_owned());
        }
    }

    let variables = match object.get("variables") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) if items.iter().all(is_workflow_variable) => {
            items.iter().map(to_variable).collect()
        }
        Some(_) => {
            return Err(
                "Invalid workflow field: variables must be an array of variable objects".to_owned(),
            )
        }
    };

    let steps = match object.get("steps") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) if items.iter().all(is_workflow_step) => {
            items.iter().map(to_step).collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(STEP_ERROR.to_owned()),
    };

    let blocks = parse_workflow_blocks(object.get("blocks"))?;

    let output = object
        .get("output")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let error_handling: Option<ErrorHandlingConfig> = object
        .get("errorHandling")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let schedule = object
        .get("schedule")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let visibility = match text_field(object, "visibility").as_deref() {
        Some("user") => Some(WorkflowVisibility::User),
        Some("dashboardHidden") => Some(WorkflowVisibility::DashboardHidden),
        _ => None,
    };
    let timeout_secs = object
        .get("timeoutSecs")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());

    Ok(WorkflowDefinition {
        id: text_field(object, "id").unwrap_or_default(),
        name: text_field(object, "name").unwrap_or_default(),
        description: text_field(object, "description").unwrap_or_default(),
        version: text_field(object, "version"),
        author: text_field(object, "author"),
        variables,
        connection: text_field(object, "connection"),
        database: text_field(object, "database"),
        steps,
        blocks: Some(blocks),
        output,
        timeout_secs,
        error_handling,
        schedule,
        visibility,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn json_parse(yaml: &str) -> Result<Value, String> {
        serde_json::from_str(yaml).map_err(|error| error.to_string())
    }

    #[test]
    fn yaml_parse_seam_requires_an_object() {
        let parsed = parse_workflow_yaml(r#"{"id":"w"}"#, json_parse).unwrap();
        assert_eq!(parsed["id"], "w");
        assert_eq!(
            parse_workflow_yaml("[]", json_parse),
            Err("YAML must be an object".to_owned())
        );
        assert_eq!(
            parse_workflow_yaml("bad", |_| Err("boom".to_owned())),
            Err("boom".to_owned())
        );
    }

    #[test]
    fn blocks_default_to_empty_and_validate_each_field() {
        assert_eq!(parse_workflow_blocks(None).unwrap(), Vec::new());
        assert_eq!(
            parse_workflow_blocks(Some(&json!("x"))).unwrap_err(),
            "Invalid workflow field: blocks must be an array of block objects"
        );
        assert_eq!(
            parse_workflow_blocks(Some(&json!([1]))).unwrap_err(),
            "Invalid workflow field: blocks[0] must be an object"
        );
        assert_eq!(
            parse_workflow_blocks(Some(
                &json!([{"type":"sessionBlock","steps":[{"id":"s","type":"query"}]}])
            ))
            .unwrap_err(),
            "Invalid workflow field: blocks[0].id must be a non-empty string"
        );
        assert_eq!(
            parse_workflow_blocks(Some(&json!([
                {"id":"b","type":"sessionBlock","steps":[{"id":"s","type":"query"}]},
                {"id":"b","type":"sessionBlock","steps":[{"id":"s","type":"query"}]}
            ])))
            .unwrap_err(),
            "Invalid workflow field: blocks[1].id duplicates block 'b'"
        );
        assert_eq!(
            parse_workflow_blocks(Some(
                &json!([{"id":"b","type":"other","steps":[{"id":"s","type":"query"}]}])
            ))
            .unwrap_err(),
            "Invalid workflow field: blocks[0].type must be sessionBlock or transactionBlock"
        );
        assert_eq!(
            parse_workflow_blocks(Some(&json!([{"id":"b","type":"sessionBlock","steps":[]}])))
                .unwrap_err(),
            "Invalid workflow field: blocks[0].steps must be a non-empty array of step objects"
        );
        assert_eq!(
            parse_workflow_blocks(Some(&json!([{"id":"b","type":"sessionBlock","steps":[{"id":"s","type":"query"}],"parallel":"yes"}]))).unwrap_err(),
            "Invalid workflow field: blocks[0].parallel must be a boolean"
        );
        assert_eq!(
            parse_workflow_blocks(Some(&json!([{"id":"b","type":"sessionBlock","steps":[{"id":"s","type":"query"}],"retry":{"block":-1}}]))).unwrap_err(),
            "Invalid workflow field: blocks[0].retry must be { block: <non-negative integer> }"
        );
    }

    #[test]
    fn valid_block_keeps_target_flags_steps_and_retry() {
        let blocks = parse_workflow_blocks(Some(&json!([{
            "id": "b", "type": "transactionBlock", "connection": "c", "database": "d",
            "parallel": true, "retry": {"block": 2},
            "steps": [{"id": "s", "type": "query", "sql": "select 1"}]
        }])))
        .unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "b");
        assert_eq!(blocks[0].r#type, WorkflowBlockType::TransactionBlock);
        assert_eq!(blocks[0].connection.as_deref(), Some("c"));
        assert_eq!(blocks[0].database.as_deref(), Some("d"));
        assert_eq!(blocks[0].parallel, Some(true));
        assert_eq!(blocks[0].retry, Some(WorkflowBlockRetry { block: 2 }));
        assert_eq!(blocks[0].steps[0].id, "s");
    }

    #[test]
    fn definition_reports_first_shape_error() {
        assert_eq!(
            parse_validated_workflow_definition(&json!({})).unwrap_err(),
            "Missing required workflow field: id"
        );
        let base = json!({"id":"w","name":"n","steps":[{"id":"s","type":"query"}]});
        let mut bad = base.clone();
        bad["description"] = json!(3);
        assert_eq!(
            parse_validated_workflow_definition(&bad).unwrap_err(),
            "Invalid workflow field: description must be a string"
        );
        let mut bad = base.clone();
        bad["variables"] = json!([{"name":"x"}]);
        assert_eq!(
            parse_validated_workflow_definition(&bad).unwrap_err(),
            "Invalid workflow field: variables must be an array of variable objects"
        );
        let mut bad = base.clone();
        bad["steps"] = json!([{"id":"s"}]);
        assert_eq!(
            parse_validated_workflow_definition(&bad).unwrap_err(),
            STEP_ERROR
        );
    }

    #[test]
    fn definition_builds_defaults_and_optional_legs() {
        let definition = parse_validated_workflow_definition(&json!({
            "id": "w", "name": "n",
            "variables": [{"name":"q","varType":"string","description":"query text","required":true}],
            "steps": [{"id":"s","type":"query","sql":"select 1"}],
            "blocks": [{"id":"b","type":"sessionBlock","steps":[{"id":"s2","type":"command"}]}],
            "visibility": "dashboardHidden", "timeoutSecs": 30
        })).unwrap();
        assert_eq!(definition.id, "w");
        assert_eq!(definition.description, "");
        assert_eq!(definition.variables[0].r#type, "string");
        assert_eq!(definition.steps[0].sql.as_deref(), Some("select 1"));
        assert!(definition.blocks.is_some());
        assert_eq!(
            definition.visibility,
            Some(WorkflowVisibility::DashboardHidden)
        );
        assert_eq!(definition.timeout_secs, Some(30));
    }

    #[test]
    fn blocks_only_definition_is_valid_v2() {
        let definition = parse_validated_workflow_definition(&json!({
            "id": "w", "name": "n", "steps": [],
            "blocks": [{"id":"b","type":"sessionBlock","steps":[{"id":"s","type":"query"}]}]
        }))
        .unwrap();
        assert!(definition.steps.is_empty());
        assert_eq!(definition.blocks.map(|blocks| blocks.len()), Some(1));
    }
}
