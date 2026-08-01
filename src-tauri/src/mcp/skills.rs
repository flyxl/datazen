//! User-defined AI workflow (Skill) system.
//!
//! Skills are YAML-defined reusable workflows combining prompt templates,
//! database queries, and variable substitution.

use crate::commands::AppState;
use datazen_ai_api::{ChatMessage, CompletionRequest, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    #[serde(default)]
    pub variables: Vec<SkillVariable>,
    pub steps: Vec<SkillStep>,
    pub output: Option<SkillOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVariable {
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: String,
    pub description: String,
    pub required: Option<bool>,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SkillStep {
    #[serde(rename = "query")]
    Query { id: String, sql: String },
    #[serde(rename = "ai")]
    Ai { id: String, prompt: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub format: String,
    pub template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<SkillVariable>,
}

pub struct SkillRegistry {
    skills: RwLock<HashMap<String, SkillDefinition>>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            skills: RwLock::new(HashMap::new()),
            skills_dir,
        }
    }

    pub async fn load_all(&self) -> Result<(), String> {
        if !self.skills_dir.exists() {
            std::fs::create_dir_all(&self.skills_dir).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let mut skills = self.skills.write().await;
        skills.clear();

        let entries = std::fs::read_dir(&self.skills_dir).map_err(|e| e.to_string())?;

        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path
                .extension()
                .map_or(false, |ext| ext == "yaml" || ext == "yml")
            {
                match Self::load_skill_file(&path) {
                    Ok(skill) => {
                        tracing::info!("Loaded skill: {} ({})", skill.name, skill.id);
                        skills.insert(skill.id.clone(), skill);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load skill {:?}: {}", path, e);
                    }
                }
            }
        }

        tracing::info!("Loaded {} skills", skills.len());
        Ok(())
    }

    fn load_skill_file(path: &std::path::Path) -> Result<SkillDefinition, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
        serde_yaml::from_str::<SkillDefinition>(&content)
            .map_err(|e| format!("Failed to parse {path:?}: {e}"))
    }

    pub async fn get(&self, id: &str) -> Option<SkillDefinition> {
        self.skills.read().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<SkillListItem> {
        self.skills
            .read()
            .await
            .values()
            .map(|s| SkillListItem {
                id: s.id.clone(),
                name: s.name.clone(),
                description: s.description.clone(),
                variables: s.variables.clone(),
            })
            .collect()
    }

    fn validate_id(id: &str) -> Result<(), String> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("Invalid skill id: {id}. Only alphanumeric, dash, and underscore are allowed."));
        }
        Ok(())
    }

    pub async fn save_skill(&self, skill: &SkillDefinition) -> Result<(), String> {
        Self::validate_id(&skill.id)?;
        let yaml = serde_yaml::to_string(skill).map_err(|e| e.to_string())?;
        let path = self.skills_dir.join(format!("{}.yaml", skill.id));
        std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
        self.skills
            .write()
            .await
            .insert(skill.id.clone(), skill.clone());
        Ok(())
    }

    pub async fn delete_skill(&self, id: &str) -> Result<(), String> {
        Self::validate_id(id)?;
        let path = self.skills_dir.join(format!("{id}.yaml"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        self.skills.write().await.remove(id);
        Ok(())
    }
}

pub struct SkillExecutor;

impl SkillExecutor {
    pub async fn execute(
        skill: &SkillDefinition,
        app_state: &AppState,
        connection_id: Option<&str>,
        variables: &serde_json::Value,
    ) -> Result<String, String> {
        let mut context = SkillContext::new(variables);
        context.set_builtin_variables();

        for var in &skill.variables {
            if !context.variables.contains_key(&var.name) {
                if let Some(default) = &var.default {
                    let val = match default {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    context.variables.insert(var.name.clone(), val);
                }
            }
            if var.required.unwrap_or(false)
                && context
                    .variables
                    .get(&var.name)
                    .map_or(true, |v| v.is_empty())
            {
                return Err(format!("Required variable '{}' is missing", var.name));
            }
        }

        for step in &skill.steps {
            match step {
                SkillStep::Query { id, sql } => {
                    let conn_id = connection_id
                        .ok_or("Skill requires a database connection")?;

                    let resolved_sql = context.resolve_template(sql)?;
                    let (driver, handle) = app_state
                        .connection_manager
                        .get_connection(conn_id)
                        .await
                        .map_err(|e| e.to_string())?;

                    let limited_sql = if !resolved_sql.to_uppercase().contains("LIMIT") {
                        format!("{resolved_sql} LIMIT 1000")
                    } else {
                        resolved_sql.clone()
                    };
                    let result = driver
                        .query(&handle, &limited_sql)
                        .await
                        .map_err(|e| e.to_string())?;

                    let result_str =
                        serde_json::to_string_pretty(&result.rows).unwrap_or_default();
                    context.set_step_result(id, &result_str);
                }

                SkillStep::Ai { id, prompt } => {
                    let resolved_prompt = context.resolve_template(prompt)?;

                    let ai_config = app_state
                        .store
                        .get_ai_config()
                        .await
                        .ok_or("AI not configured")?;

                    let provider = app_state
                        .ai_registry
                        .get(&ai_config.provider_type)
                        .await
                        .ok_or("AI provider not available")?;

                    let request = CompletionRequest {
                        request_id: Uuid::new_v4().to_string(),
                        model: ai_config.model.clone(),
                        messages: vec![ChatMessage {
                            role: MessageRole::User,
                            content: resolved_prompt,
                        }],
                        temperature: Some(0.3),
                        max_tokens: Some(4000),
                        stop: None,
                    };

                    let response = provider.complete(&request).await.map_err(|e| e.to_string())?;
                    context.set_step_result(id, &response.content);
                }
            }
        }

        if let Some(ref output) = skill.output {
            if let Some(ref template) = output.template {
                context.resolve_template(template)
            } else {
                Ok(context.get_last_result().unwrap_or_default())
            }
        } else {
            Ok(context.get_last_result().unwrap_or_default())
        }
    }
}

struct SkillContext {
    variables: HashMap<String, String>,
    step_results: HashMap<String, String>,
    last_step_id: Option<String>,
}

impl SkillContext {
    fn new(input: &serde_json::Value) -> Self {
        let mut variables = HashMap::new();
        if let Some(obj) = input.as_object() {
            for (k, v) in obj {
                variables.insert(
                    k.clone(),
                    match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                );
            }
        }
        Self {
            variables,
            step_results: HashMap::new(),
            last_step_id: None,
        }
    }

    fn set_builtin_variables(&mut self) {
        let now = chrono::Local::now();
        self.variables
            .insert("current_month".into(), now.format("%Y-%m").to_string());
        self.variables
            .insert("current_date".into(), now.format("%Y-%m-%d").to_string());
        self.variables
            .insert("current_year".into(), now.format("%Y").to_string());
    }

    fn set_step_result(&mut self, step_id: &str, result: &str) {
        self.step_results.insert(step_id.into(), result.into());
        self.last_step_id = Some(step_id.into());
    }

    fn get_last_result(&self) -> Option<String> {
        self.last_step_id
            .as_ref()
            .and_then(|id| self.step_results.get(id).cloned())
    }

    fn resolve_template(&self, template: &str) -> Result<String, String> {
        let mut result = template.to_string();

        let step_re =
            regex::Regex::new(r"\{\{steps\.([a-zA-Z0-9_-]+)\.result\}\}").map_err(|e| e.to_string())?;
        result = step_re
            .replace_all(&result, |caps: &regex::Captures| {
                let step_id = &caps[1];
                self.step_results.get(step_id).cloned().unwrap_or_default()
            })
            .to_string();

        let var_re = regex::Regex::new(r"\{\{(\w+)\}\}").map_err(|e| e.to_string())?;
        result = var_re
            .replace_all(&result, |caps: &regex::Captures| {
                let var_name = &caps[1];
                self.variables.get(var_name).cloned().unwrap_or_default()
            })
            .to_string();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_definition_yaml_parsing() {
        let yaml = r#"
id: test-skill
name: Test Skill
description: A test skill
variables:
  - name: table_name
    type: string
    description: Table to query
    required: true
steps:
  - type: query
    id: get_data
    sql: "SELECT * FROM {{table_name}} LIMIT 10"
  - type: ai
    id: analyze
    prompt: "Analyze this data: {{steps.get_data.result}}"
output:
  format: markdown
"#;
        let skill: SkillDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(skill.id, "test-skill");
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.variables.len(), 1);
        assert_eq!(skill.steps.len(), 2);
        assert!(skill.output.is_some());
    }

    #[test]
    fn test_skill_context_variable_resolution() {
        let input = serde_json::json!({"table_name": "users", "limit": 10});
        let mut ctx = SkillContext::new(&input);
        ctx.set_builtin_variables();

        let result = ctx.resolve_template("SELECT * FROM {{table_name}} LIMIT {{limit}}").unwrap();
        assert_eq!(result, "SELECT * FROM users LIMIT 10");
    }

    #[test]
    fn test_skill_context_step_result_resolution() {
        let input = serde_json::json!({});
        let mut ctx = SkillContext::new(&input);
        ctx.set_step_result("query1", "[{\"id\": 1}]");

        let result = ctx
            .resolve_template("Data: {{steps.query1.result}}")
            .unwrap();
        assert_eq!(result, "Data: [{\"id\": 1}]");
    }

    #[test]
    fn test_skill_context_builtin_variables() {
        let input = serde_json::json!({});
        let mut ctx = SkillContext::new(&input);
        ctx.set_builtin_variables();

        let result = ctx.resolve_template("Date: {{current_date}}").unwrap();
        assert!(result.starts_with("Date: 20"));
        assert!(result.len() > 10);
    }

    #[test]
    fn test_skill_list_item_serialization() {
        let item = SkillListItem {
            id: "test".into(),
            name: "Test".into(),
            description: "A test".into(),
            variables: vec![],
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"id\":\"test\""));
    }

    #[test]
    fn test_skill_context_hyphenated_step_id() {
        let input = serde_json::json!({});
        let mut ctx = SkillContext::new(&input);
        ctx.set_step_result("get-data", "result here");

        let result = ctx
            .resolve_template("Output: {{steps.get-data.result}}")
            .unwrap();
        assert_eq!(result, "Output: result here");
    }

    #[test]
    fn test_skill_context_last_result_ordering() {
        let input = serde_json::json!({});
        let mut ctx = SkillContext::new(&input);
        ctx.set_step_result("step1", "first");
        ctx.set_step_result("step2", "second");
        ctx.set_step_result("step3", "third");
        assert_eq!(ctx.get_last_result(), Some("third".to_string()));
    }

    #[test]
    fn test_skill_id_validation() {
        assert!(SkillRegistry::validate_id("valid-id_123").is_ok());
        assert!(SkillRegistry::validate_id("").is_err());
        assert!(SkillRegistry::validate_id("../../evil").is_err());
        assert!(SkillRegistry::validate_id("has space").is_err());
        assert!(SkillRegistry::validate_id("path/slash").is_err());
    }

    #[test]
    fn test_skill_step_enum_deserialization() {
        let yaml = r#"type: query
id: get_data
sql: "SELECT 1""#;
        let step: SkillStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            SkillStep::Query { id, sql } => {
                assert_eq!(id, "get_data");
                assert_eq!(sql, "SELECT 1");
            }
            _ => panic!("Expected Query step"),
        }

        let yaml = r#"type: ai
id: analyze
prompt: "Analyze this""#;
        let step: SkillStep = serde_yaml::from_str(yaml).unwrap();
        match step {
            SkillStep::Ai { id, prompt } => {
                assert_eq!(id, "analyze");
                assert_eq!(prompt, "Analyze this");
            }
            _ => panic!("Expected Ai step"),
        }
    }
}
