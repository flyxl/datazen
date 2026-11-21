use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::WorkflowStepType;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkflowDraftVariable {
    pub name: String,
    pub var_type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowStepDraft {
    pub step_type: WorkflowStepType,
    pub id: String,
    pub sql: Option<String>,
    pub prompt: Option<String>,
    pub connection: Option<String>,
    pub database: Option<String>,
    pub command: Option<String>,
    pub input: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkflowDraft {
    pub id: String,
    pub name: String,
    pub description: String,
    pub connection: Option<String>,
    pub database: Option<String>,
    pub variables: Vec<WorkflowDraftVariable>,
    pub steps: Vec<WorkflowStepDraft>,
    pub schedule_enabled: bool,
    pub schedule_interval_secs: Option<i64>,
}

pub const WORKFLOW_DRAFT_YAML_GAP: [&str; 2] = [
    "YAML.stringify / YAML.parse around the draft object",
    "WorkflowForm draft state and editor bindings",
];

pub const WORKFLOW_DRAFT_YAML_SURFACE: [&str; 5] = [
    "draftToYamlObject",
    "yamlObjectToDraft",
    "WorkflowDraft",
    "WorkflowStepDraft",
    "WORKFLOW_DRAFT_YAML_GAP",
];

fn step_type_from(value: Option<&Value>) -> WorkflowStepType {
    let text = match value {
        Some(Value::String(text)) => text.as_str(),
        _ => "query",
    };
    [
        WorkflowStepType::Query,
        WorkflowStepType::Command,
        WorkflowStepType::Ai,
        WorkflowStepType::Condition,
        WorkflowStepType::Foreach,
        WorkflowStepType::Merge,
        WorkflowStepType::Transform,
    ]
    .iter()
    .find(|kind| kind.as_str() == text)
    .copied()
    .unwrap_or(WorkflowStepType::Query)
}

fn js_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(boolean)) => boolean.to_string(),
        Some(other) => other.to_string(),
    }
}

fn non_null_string(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(other) => Some(js_string(Some(other))),
    }
}

fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(boolean)) => *boolean,
        Some(Value::Number(number)) => number.as_f64().map(|value| value != 0.0).unwrap_or(false),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::Object(entries)) => !entries.is_empty(),
    }
}

fn as_object_map(value: Option<&Value>) -> BTreeMap<String, Value> {
    value
        .and_then(Value::as_object)
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string(target: &mut BTreeMap<String, Value>, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        target.insert(key.to_owned(), Value::String(value.clone()));
    }
}

fn trim_or(target: &str, fallback: &str) -> String {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub fn draft_to_yaml_object(draft: &WorkflowDraft) -> BTreeMap<String, Value> {
    let mut object = BTreeMap::new();
    object.insert(
        "id".to_owned(),
        Value::String(trim_or(&draft.id, "new-workflow")),
    );
    object.insert(
        "name".to_owned(),
        Value::String(trim_or(&draft.name, "New Workflow")),
    );
    object.insert(
        "description".to_owned(),
        Value::String(draft.description.trim().to_owned()),
    );
    let variables = draft
        .variables
        .iter()
        .map(|variable| {
            json!({
                "name": variable.name,
                "type": variable.var_type,
                "description": variable.description,
                "required": variable.required,
            })
        })
        .collect::<Vec<_>>();
    object.insert("variables".to_owned(), Value::Array(variables));
    let steps = draft
        .steps
        .iter()
        .map(step_to_yaml_object)
        .collect::<Vec<_>>();
    object.insert("steps".to_owned(), Value::Array(steps));
    optional_string(&mut object, "connection", &draft.connection);
    optional_string(&mut object, "database", &draft.database);
    if draft.schedule_enabled {
        object.insert(
            "schedule".to_owned(),
            json!({"enabled": true, "interval_secs": draft.schedule_interval_secs.unwrap_or(3600).max(30)}),
        );
    } else {
        object.insert("schedule".to_owned(), json!({"enabled": false}));
    }
    object
}

fn step_to_yaml_object(step: &WorkflowStepDraft) -> Value {
    let mut object = BTreeMap::new();
    object.insert(
        "type".to_owned(),
        Value::String(step.step_type.as_str().to_owned()),
    );
    object.insert("id".to_owned(), Value::String(step.id.clone()));
    if step.step_type == WorkflowStepType::Command {
        object.insert(
            "command".to_owned(),
            Value::String(step.command.clone().unwrap_or_default()),
        );
        optional_string(&mut object, "connection", &step.connection);
        let mut input = step.input.clone().unwrap_or_default();
        if let Some(database) = &step.database {
            input.insert("database".to_owned(), Value::String(database.clone()));
        }
        object.insert(
            "input".to_owned(),
            Value::Object(input.into_iter().collect::<Map<String, Value>>()),
        );
    } else {
        optional_string(&mut object, "sql", &step.sql);
        optional_string(&mut object, "prompt", &step.prompt);
        optional_string(&mut object, "connection", &step.connection);
        optional_string(&mut object, "database", &step.database);
    }
    Value::Object(object.into_iter().collect::<Map<String, Value>>())
}

pub fn draft_to_yaml_value(draft: &WorkflowDraft) -> Value {
    Value::Object(draft_to_yaml_object(draft).into_iter().collect())
}

pub fn yaml_object_to_draft(object: &BTreeMap<String, Value>) -> WorkflowDraft {
    let schedule = object.get("schedule").and_then(Value::as_object);
    let variables = object
        .get("variables")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|item| WorkflowDraftVariable {
                    name: js_string(item.get("name")),
                    var_type: js_string(item.get("type")),
                    description: js_string(item.get("description")),
                    required: js_truthy(item.get("required")),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let steps = object
        .get("steps")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(step_from_object)
                .collect()
        })
        .unwrap_or_default();
    let schedule_enabled = schedule
        .and_then(|entry| entry.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let schedule_interval_secs = schedule
        .and_then(|entry| entry.get("interval_secs"))
        .and_then(interval_number)
        .or(Some(3600));
    WorkflowDraft {
        id: js_string(object.get("id")),
        name: js_string(object.get("name")),
        description: js_string(object.get("description")),
        connection: non_null_string(object.get("connection")),
        database: non_null_string(object.get("database")),
        variables,
        steps,
        schedule_enabled,
        schedule_interval_secs,
    }
}

fn interval_number(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

fn step_from_object(object: &Map<String, Value>) -> WorkflowStepDraft {
    let input = as_object_map(object.get("input"));
    let database = object
        .get("database")
        .and_then(|value| non_null_string(Some(value)))
        .or_else(|| {
            input
                .get("database")
                .and_then(|value| non_null_string(Some(value)))
        });
    WorkflowStepDraft {
        step_type: step_type_from(object.get("type")),
        id: js_string(object.get("id")),
        sql: Some(js_string(object.get("sql"))),
        prompt: Some(js_string(object.get("prompt"))),
        connection: non_null_string(object.get("connection")),
        database,
        command: Some(js_string(object.get("command"))),
        input: Some(input),
    }
}

pub fn yaml_value_to_draft(value: &Value) -> WorkflowDraft {
    match value.as_object() {
        Some(object) => yaml_object_to_draft(
            &object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        None => WorkflowDraft::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_step() -> WorkflowStepDraft {
        WorkflowStepDraft {
            step_type: WorkflowStepType::Command,
            id: "step-1".to_owned(),
            sql: None,
            prompt: None,
            connection: Some("conn".to_owned()),
            database: Some("db2".to_owned()),
            command: Some("list_tables".to_owned()),
            input: Some(BTreeMap::from([(
                "schema".to_owned(),
                Value::String("public".to_owned()),
            )])),
        }
    }

    fn query_step() -> WorkflowStepDraft {
        WorkflowStepDraft {
            step_type: WorkflowStepType::Query,
            id: "step-2".to_owned(),
            sql: Some("SELECT 1".to_owned()),
            prompt: Some("note".to_owned()),
            connection: None,
            database: Some("db1".to_owned()),
            command: None,
            input: None,
        }
    }

    fn draft() -> WorkflowDraft {
        WorkflowDraft {
            id: " wf-1 ".to_owned(),
            name: "  My Flow  ".to_owned(),
            description: "  does things  ".to_owned(),
            connection: Some("conn".to_owned()),
            database: Some("db1".to_owned()),
            variables: vec![WorkflowDraftVariable {
                name: "limit".to_owned(),
                var_type: "number".to_owned(),
                description: "how many".to_owned(),
                required: true,
            }],
            steps: vec![command_step(), query_step()],
            schedule_enabled: true,
            schedule_interval_secs: Some(5),
        }
    }

    #[test]
    fn draft_serializes_with_trimmed_defaults_and_variable_legs() {
        let object = draft_to_yaml_object(&draft());
        assert_eq!(object["id"], json!("wf-1"));
        assert_eq!(object["name"], json!("My Flow"));
        assert_eq!(object["description"], json!("does things"));
        assert_eq!(object["variables"][0]["name"], json!("limit"));
        assert_eq!(object["variables"][0]["type"], json!("number"));
        assert_eq!(object["variables"][0]["required"], json!(true));
        assert_eq!(object["connection"], json!("conn"));
        assert_eq!(object["database"], json!("db1"));
    }

    #[test]
    fn blank_ids_and_names_use_the_react_fallbacks() {
        let mut input = draft();
        input.id = "   ".to_owned();
        input.name = String::new();
        input.description = "  x  ".to_owned();
        let object = draft_to_yaml_object(&input);
        assert_eq!(object["id"], json!("new-workflow"));
        assert_eq!(object["name"], json!("New Workflow"));
        assert_eq!(object["description"], json!("x"));
    }

    #[test]
    fn command_step_keeps_database_inside_input_only() {
        let object = draft_to_yaml_object(&draft());
        let command = &object["steps"][0];
        assert_eq!(command["type"], json!("command"));
        assert_eq!(command["command"], json!("list_tables"));
        assert_eq!(command["input"]["schema"], json!("public"));
        assert_eq!(command["input"]["database"], json!("db2"));
        assert!(command.get("database").is_none());
        assert!(command.get("sql").is_none());
    }

    #[test]
    fn command_step_without_database_or_command_uses_empty_shape() {
        let step = WorkflowStepDraft {
            step_type: WorkflowStepType::Command,
            id: "s".to_owned(),
            command: None,
            database: None,
            input: None,
            ..command_step()
        };
        let object = step_to_yaml_object(&step);
        assert_eq!(object["command"], json!(""));
        assert_eq!(object["input"], json!({}));
        assert!(object.get("database").is_none());
    }

    #[test]
    fn query_step_carries_its_own_database_leg() {
        let object = draft_to_yaml_object(&draft());
        let query = &object["steps"][1];
        assert_eq!(query["type"], json!("query"));
        assert_eq!(query["sql"], json!("SELECT 1"));
        assert_eq!(query["database"], json!("db1"));
        assert!(query.get("input").is_none());
        assert!(query.get("connection").is_none());
    }

    #[test]
    fn schedule_emits_a_clamped_interval_only_when_enabled() {
        let enabled = draft_to_yaml_object(&draft());
        assert_eq!(
            enabled["schedule"],
            json!({"enabled": true, "interval_secs": 30})
        );
        let mut input = draft();
        input.schedule_interval_secs = None;
        let defaulted = draft_to_yaml_object(&input);
        assert_eq!(defaulted["schedule"]["interval_secs"], json!(3600));
        input.schedule_enabled = false;
        let disabled = draft_to_yaml_object(&input);
        assert_eq!(disabled["schedule"], json!({"enabled": false}));
        assert!(disabled["schedule"].get("interval_secs").is_none());
    }

    #[test]
    fn yaml_object_to_draft_coerces_legs_and_defaults_arrays() {
        let value = json!({
            "id": 7,
            "name": null,
            "description": "d",
            "connection": null,
            "database": "db",
            "variables": [{"name": "v", "type": "string", "description": "D", "required": "yes"}],
            "steps": [{"type": "ai", "id": "a", "sql": 1, "prompt": true, "command": null, "input": {"k": 1}}],
            "schedule": {"enabled": true, "interval_secs": 120},
        });
        let object = value
            .as_object()
            .map(|entries| {
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<BTreeMap<String, Value>>()
            })
            .unwrap_or_default();
        let result = yaml_object_to_draft(&object);
        assert_eq!(result.id, "7");
        assert_eq!(result.name, "");
        assert_eq!(result.connection, None);
        assert_eq!(result.database, Some("db".to_owned()));
        assert_eq!(result.variables[0].var_type, "string");
        assert!(result.variables[0].required);
        assert_eq!(result.steps[0].step_type, WorkflowStepType::Ai);
        assert_eq!(result.steps[0].sql.as_deref(), Some("1"));
        assert_eq!(result.steps[0].prompt.as_deref(), Some("true"));
        assert_eq!(result.steps[0].command.as_deref(), Some(""));
        assert_eq!(result.schedule_interval_secs, Some(120));
        assert!(result.schedule_enabled);
    }

    #[test]
    fn yaml_object_to_draft_falls_back_for_missing_and_malformed_legs() {
        let value = json!({
            "steps": [{"type": "not-a-kind", "id": "s", "database": "top", "input": {"database": "inner"}}],
        });
        let result = yaml_value_to_draft(&value);
        assert_eq!(result.id, "");
        assert!(result.variables.is_empty());
        assert_eq!(result.steps[0].step_type, WorkflowStepType::Query);
        assert_eq!(result.steps[0].database.as_deref(), Some("top"));
        assert_eq!(result.steps[0].sql.as_deref(), Some(""));
        assert_eq!(result.steps[0].input.as_ref().map(BTreeMap::len), Some(1));
        assert!(!result.schedule_enabled);
        assert_eq!(result.schedule_interval_secs, Some(3600));

        let missing = yaml_value_to_draft(&Value::Null);
        assert_eq!(missing, WorkflowDraft::default());
    }

    #[test]
    fn command_step_database_is_recovered_from_input() {
        let value = json!({
            "steps": [{"type": "command", "id": "c", "input": {"database": "from-input", "x": 1}}],
        });
        let result = yaml_value_to_draft(&value);
        assert_eq!(result.steps[0].database.as_deref(), Some("from-input"));
        assert_eq!(result.steps[0].input.as_ref().map(BTreeMap::len), Some(2));
    }

    #[test]
    fn yaml_object_to_draft_ignores_non_array_and_non_object_legs() {
        let value = json!({"variables": 5, "steps": {"not": "array"}});
        let result = yaml_value_to_draft(&value);
        assert!(result.variables.is_empty());
        assert!(result.steps.is_empty());
    }

    #[test]
    fn draft_round_trips_the_yaml_object() {
        let mut command = command_step();
        command.sql = Some(String::new());
        command.prompt = Some(String::new());
        command.input = Some(BTreeMap::from([
            ("schema".to_owned(), Value::String("public".to_owned())),
            ("database".to_owned(), Value::String("db2".to_owned())),
        ]));
        let query = WorkflowStepDraft {
            command: Some(String::new()),
            input: Some(BTreeMap::new()),
            ..query_step()
        };
        let original = WorkflowDraft {
            id: "wf-1".to_owned(),
            name: "My Flow".to_owned(),
            description: "does things".to_owned(),
            steps: vec![command, query],
            schedule_interval_secs: Some(30),
            ..draft()
        };
        let object = draft_to_yaml_object(&original);
        let restored = yaml_value_to_draft(&Value::Object(object.into_iter().collect()));
        assert_eq!(restored, original);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(WORKFLOW_DRAFT_YAML_GAP.len(), 2);
        assert_eq!(WORKFLOW_DRAFT_YAML_SURFACE.len(), 5);
        assert_eq!(crate::WORKFLOW_DRAFT_YAML_GAP.len(), 2);
        assert_eq!(crate::WORKFLOW_DRAFT_YAML_SURFACE.len(), 5);
        let _: Option<crate::WorkflowDraft> = Some(WorkflowDraft::default());
        let _: Option<crate::WorkflowStepDraft> = Some(query_step());
    }
}
