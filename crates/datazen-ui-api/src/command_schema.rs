use serde_json::{Map, Value};

use crate::types::DriverCommandDefinition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaField {
    pub name: String,
    pub schema: Value,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivilegeGroup {
    pub label: String,
    pub privileges: Vec<String>,
}

fn as_object(value: Option<&Value>) -> Option<&Map<String, Value>> {
    match value {
        Some(Value::Object(map)) => Some(map),
        _ => None,
    }
}

pub fn schema_properties(definition: Option<&DriverCommandDefinition>) -> Vec<SchemaField> {
    let Some(schema) = definition.and_then(|d| as_object(Some(&d.input_schema))) else {
        return Vec::new();
    };
    let Some(properties) = schema.get("properties").and_then(|v| as_object(Some(v))) else {
        return Vec::new();
    };
    let required = schema.get("required").and_then(Value::as_array);
    properties
        .iter()
        .map(|(name, raw)| SchemaField {
            name: name.clone(),
            schema: match raw {
                Value::Object(_) | Value::Array(_) => raw.clone(),
                _ => Value::Object(Map::new()),
            },
            required: required.is_some_and(|list| {
                list.iter()
                    .any(|entry| entry.as_str() == Some(name.as_str()))
            }),
        })
        .collect()
}

pub fn has_schema_field(definition: Option<&DriverCommandDefinition>, name: &str) -> bool {
    schema_properties(definition)
        .iter()
        .any(|field| field.name == name)
}

pub fn get_schema_field(
    definition: Option<&DriverCommandDefinition>,
    name: &str,
) -> Option<SchemaField> {
    schema_properties(definition)
        .into_iter()
        .find(|field| field.name == name)
}

pub fn field_placeholder(field: &SchemaField) -> Option<String> {
    if let Some(examples) = field.schema.get("examples").and_then(Value::as_array) {
        if let Some(first) = examples.first().and_then(Value::as_str) {
            return Some(first.to_owned());
        }
    }
    field
        .schema
        .get("example")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn field_label(field: &SchemaField) -> String {
    for key in ["title", "description"] {
        if let Some(value) = field.schema.get(key).and_then(Value::as_str) {
            if !value.is_empty() {
                return value.to_owned();
            }
        }
    }
    field.name.clone()
}

fn read_datazen_extension(
    definition: Option<&DriverCommandDefinition>,
) -> Option<&Map<String, Value>> {
    let schema = definition.and_then(|d| as_object(Some(&d.input_schema)))?;
    as_object(schema.get("x-datazen"))
}

pub fn privilege_groups(definition: Option<&DriverCommandDefinition>) -> Vec<PrivilegeGroup> {
    let Some(ext) = read_datazen_extension(definition) else {
        return Vec::new();
    };
    let Some(raw) = ext.get("privilegeGroups").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for item in raw {
        let Some(record) = as_object(Some(item)) else {
            continue;
        };
        let privileges: Vec<String> = record
            .get("privileges")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if privileges.is_empty() {
            continue;
        }
        let label = record
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        groups.push(PrivilegeGroup { label, privileges });
    }
    groups
}

pub fn all_privileges(definition: Option<&DriverCommandDefinition>) -> Vec<String> {
    privilege_groups(definition)
        .into_iter()
        .flat_map(|group| group.privileges)
        .collect()
}

pub fn has_command(definitions: Option<&[DriverCommandDefinition]>, command_id: &str) -> bool {
    definitions.is_some_and(|list| list.iter().any(|definition| definition.id == command_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(input_schema: Value) -> DriverCommandDefinition {
        serde_json::from_value(serde_json::json!({
            "id": "cmd-1",
            "name": "Command One",
            "inputSchema": input_schema,
            "permissions": [],
            "metadata": {
                "category": "query",
                "workflow": false,
                "ui": false,
                "deprecated": false,
                "requiresConnection": false
            }
        }))
        .unwrap()
    }

    fn basic() -> DriverCommandDefinition {
        definition(serde_json::json!({
            "type": "object",
            "properties": {
                "host": { "type": "string", "title": "Host" },
                "port": { "type": "integer" },
                "odd": 7,
                "nil": null
            },
            "required": ["host"]
        }))
    }

    #[test]
    fn empty_and_malformed_schemas_yield_no_fields() {
        assert!(schema_properties(None).is_empty());
        assert!(schema_properties(Some(&definition(serde_json::json!(null)))).is_empty());
        assert!(schema_properties(Some(&definition(serde_json::json!("text")))).is_empty());
        assert!(schema_properties(Some(&definition(serde_json::json!([])))).is_empty());
        assert!(schema_properties(Some(&definition(serde_json::json!({})))).is_empty());
        assert!(schema_properties(Some(&definition(serde_json::json!({
            "properties": []
        }))))
        .is_empty());
    }

    #[test]
    fn properties_carry_name_schema_and_required_flag() {
        let fields = schema_properties(Some(&basic()));
        assert_eq!(fields.len(), 4);
        let host = fields.iter().find(|f| f.name == "host").unwrap();
        assert_eq!(
            host.schema,
            serde_json::json!({ "type": "string", "title": "Host" })
        );
        assert!(host.required);
        let port = fields.iter().find(|f| f.name == "port").unwrap();
        assert!(!port.required);
        let odd = fields.iter().find(|f| f.name == "odd").unwrap();
        assert_eq!(odd.schema, serde_json::json!({}));
        let nil = fields.iter().find(|f| f.name == "nil").unwrap();
        assert_eq!(nil.schema, serde_json::json!({}));
    }

    #[test]
    fn array_property_schema_is_kept_as_a_js_object() {
        let fields = schema_properties(Some(&definition(serde_json::json!({
            "properties": { "enum": ["a", "b"] }
        }))));
        assert_eq!(fields[0].schema, serde_json::json!(["a", "b"]));
    }

    #[test]
    fn field_lookup_predicates() {
        assert!(has_schema_field(Some(&basic()), "host"));
        assert!(!has_schema_field(Some(&basic()), "missing"));
        assert!(!has_schema_field(None, "host"));
        assert_eq!(
            get_schema_field(Some(&basic()), "host").unwrap().name,
            "host"
        );
        assert_eq!(get_schema_field(Some(&basic()), "missing"), None);
    }

    #[test]
    fn placeholder_prefers_examples_then_example() {
        let with_examples = SchemaField {
            name: "a".to_owned(),
            schema: serde_json::json!({ "examples": ["first", "second"], "example": "fallback" }),
            required: false,
        };
        assert_eq!(field_placeholder(&with_examples).as_deref(), Some("first"));
        let non_string = SchemaField {
            name: "a".to_owned(),
            schema: serde_json::json!({ "examples": [1, 2], "example": "fallback" }),
            required: false,
        };
        assert_eq!(field_placeholder(&non_string).as_deref(), Some("fallback"));
        let empty_examples = SchemaField {
            name: "a".to_owned(),
            schema: serde_json::json!({ "examples": [] }),
            required: false,
        };
        assert_eq!(field_placeholder(&empty_examples), None);
    }

    #[test]
    fn label_falls_back_title_description_name() {
        let mk = |schema: Value| SchemaField {
            name: "field".to_owned(),
            schema,
            required: false,
        };
        assert_eq!(
            field_label(&mk(
                serde_json::json!({ "title": "Title", "description": "Desc" })
            )),
            "Title"
        );
        assert_eq!(
            field_label(&mk(
                serde_json::json!({ "title": "", "description": "Desc" })
            )),
            "Desc"
        );
        assert_eq!(
            field_label(&mk(serde_json::json!({ "description": 5 }))),
            "field"
        );
        assert_eq!(field_label(&mk(serde_json::json!({}))), "field");
    }

    fn with_extension(ext: Value) -> DriverCommandDefinition {
        definition(serde_json::json!({ "x-datazen": ext }))
    }

    #[test]
    fn privilege_groups_ignore_malformed_and_empty_items() {
        assert!(privilege_groups(None).is_empty());
        assert!(privilege_groups(Some(&basic())).is_empty());
        assert!(
            privilege_groups(Some(&definition(serde_json::json!({ "x-datazen": [] })))).is_empty()
        );
        let def = with_extension(serde_json::json!({
            "privilegeGroups": [
                null,
                [],
                { "label": "No Privileges", "privileges": [] },
                { "label": "Non Strings", "privileges": ["ok", 1, null] },
                { "privileges": ["a", "b"] },
                { "label": 5, "privileges": "nope" }
            ]
        }));
        let groups = privilege_groups(Some(&def));
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "Non Strings");
        assert_eq!(groups[0].privileges, ["ok"]);
        assert_eq!(groups[1].label, "");
        assert_eq!(groups[1].privileges, ["a", "b"]);
    }

    #[test]
    fn all_privileges_flattens_in_group_order() {
        assert!(all_privileges(None).is_empty());
        let def = with_extension(serde_json::json!({
            "privilegeGroups": [
                { "label": "A", "privileges": ["a1", "a2"] },
                { "label": "B", "privileges": ["b1"] }
            ]
        }));
        assert_eq!(all_privileges(Some(&def)), ["a1", "a2", "b1"]);
    }

    #[test]
    fn has_command_matches_ids() {
        let defs = vec![basic(), definition(serde_json::json!({}))];
        assert!(has_command(Some(&defs), "cmd-1"));
        assert!(!has_command(Some(&defs), "missing"));
        assert!(!has_command(None, "cmd-1"));
    }
}
