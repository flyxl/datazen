//! Workflow execution context and template/path resolution.

use std::collections::HashMap;

pub struct WorkflowContext {
    pub variables: HashMap<String, String>,
    variable_values: HashMap<String, serde_json::Value>,
    step_results: HashMap<String, serde_json::Value>,
    loop_vars: HashMap<String, serde_json::Value>,
    last_step_id: Option<String>,
}

impl WorkflowContext {
    pub fn new(input: &serde_json::Value) -> Self {
        let mut variables = HashMap::new();
        let mut variable_values = HashMap::new();
        if let Some(obj) = input.as_object() {
            for (k, v) in obj {
                variables.insert(
                    k.clone(),
                    match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                );
                variable_values.insert(k.clone(), v.clone());
            }
        }
        Self {
            variables,
            variable_values,
            step_results: HashMap::new(),
            loop_vars: HashMap::new(),
            last_step_id: None,
        }
    }

    /// Set a variable while retaining its JSON type for SQL template rendering.
    pub fn set_variable(&mut self, name: &str, value: serde_json::Value) {
        let text = match &value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        self.variables.insert(name.into(), text);
        self.variable_values.insert(name.into(), value);
    }

    pub fn set_builtin_variables(&mut self) {
        let now = chrono::Local::now();
        self.set_variable(
            "current_month",
            serde_json::Value::String(now.format("%Y-%m").to_string()),
        );
        self.set_variable(
            "current_date",
            serde_json::Value::String(now.format("%Y-%m-%d").to_string()),
        );
        self.set_variable(
            "current_year",
            serde_json::Value::String(now.format("%Y").to_string()),
        );
        self.set_variable(
            "current_time",
            serde_json::Value::String(now.format("%H:%M:%S").to_string()),
        );
    }

    pub fn set_step_result(&mut self, step_id: &str, result: serde_json::Value) {
        self.step_results.insert(step_id.into(), result);
        self.last_step_id = Some(step_id.into());
    }

    pub fn set_loop_var(&mut self, name: &str, value: serde_json::Value) {
        self.loop_vars.insert(name.into(), value);
    }

    pub fn clear_loop_var(&mut self, name: &str) {
        self.loop_vars.remove(name);
    }

    pub fn get_last_result(&self) -> Option<String> {
        self.last_step_id.as_ref().and_then(|id| {
            self.step_results.get(id).map(|v| {
                v.get("result")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| serde_json::to_string_pretty(v).unwrap_or_default())
            })
        })
    }

    /// Resolve `{{...}}` template expressions.
    pub fn resolve_template(&self, template: &str) -> Result<String, String> {
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}").map_err(|e| e.to_string())?;

        let result = re.replace_all(template, |caps: &regex::Captures| {
            let expr = caps[1].trim();
            self.resolve_expression(expr)
        });

        Ok(result.to_string())
    }

    /// Resolve an expression to its typed JSON value for SQL rendering.
    ///
    /// Missing values are errors: a SQL template must never silently become an
    /// empty literal. `steps.*` and loop variables keep their JSON type so a
    /// number can be rendered outside a string while a string is escaped.
    pub fn resolve_expression_value(&self, expr: &str) -> Result<serde_json::Value, String> {
        if let Some(rest) = expr.strip_prefix("steps.") {
            let Some(dot_pos) = rest.find('.') else {
                return Err(format!("missing workflow template value '{expr}'"));
            };
            let step_id = &rest[..dot_pos];
            let path = &rest[dot_pos + 1..];
            let step_value = self
                .step_results
                .get(step_id)
                .ok_or_else(|| format!("missing workflow template value '{expr}'"))?;
            return Ok(self.resolve_json_path_value(step_value, path));
        }

        if let Some(separator) = expr.find(|ch| ch == '.' || ch == '[') {
            let var_name = &expr[..separator];
            if let Some(loop_val) = self.loop_vars.get(var_name) {
                let path = expr[separator..].trim_start_matches('.');
                return Ok(self.resolve_json_path_value(loop_val, path));
            }
        }

        if let Some(loop_val) = self.loop_vars.get(expr) {
            return Ok(loop_val.clone());
        }
        if let Some(value) = self.variable_values.get(expr) {
            return Ok(value.clone());
        }
        if let Some(value) = self.variables.get(expr) {
            return Ok(serde_json::Value::String(value.clone()));
        }
        Err(format!("missing workflow template value '{expr}'"))
    }

    pub fn resolve_expression(&self, expr: &str) -> String {
        if let Some(rest) = expr.strip_prefix("steps.") {
            if let Some(dot_pos) = rest.find('.') {
                let step_id = &rest[..dot_pos];
                let path = &rest[dot_pos + 1..];
                if let Some(step_val) = self.step_results.get(step_id) {
                    return self.resolve_json_path(step_val, path);
                }
            }
            return String::new();
        }

        if let Some(dot_pos) = expr.find('.') {
            let var_name = &expr[..dot_pos];
            if let Some(loop_val) = self.loop_vars.get(var_name) {
                let path = &expr[dot_pos + 1..];
                return self.resolve_json_path(loop_val, path);
            }
        }

        if let Some(loop_val) = self.loop_vars.get(expr) {
            return json_value_to_string(loop_val);
        }

        self.variables.get(expr).cloned().unwrap_or_default()
    }

    fn resolve_json_path(&self, value: &serde_json::Value, path: &str) -> String {
        json_value_to_string(&self.resolve_json_path_value(value, path))
    }

    fn resolve_json_path_value(&self, value: &serde_json::Value, path: &str) -> serde_json::Value {
        if path.contains(".*") {
            return serde_json::Value::String(self.resolve_wildcard_path(value, path));
        }

        let mut current = value.clone();
        for raw_part in path.split('.') {
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(bracket_pos) = part.find('[') {
                let field = &part[..bracket_pos];
                if !field.is_empty() {
                    let next = current
                        .get(field)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    current = if next.is_null() && (field == "data" || field == "result") {
                        current
                            .get("rows")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null)
                    } else {
                        next
                    };
                }
                let idx_str = part[bracket_pos + 1..].trim_end_matches(']');
                if let Ok(idx) = idx_str.parse::<usize>() {
                    current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                }
            } else if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
            } else {
                let next = current
                    .get(part)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (part == "data" || part == "result") {
                    current
                        .get("rows")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                } else {
                    next
                };
            }
        }

        current
    }

    fn resolve_wildcard_path(&self, value: &serde_json::Value, path: &str) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        let wildcard_pos = parts.iter().position(|p| *p == "*").unwrap_or(0);

        let mut current = value.clone();
        for part in &parts[..wildcard_pos] {
            if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
            } else {
                let next = current
                    .get(*part)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (*part == "data" || *part == "result") {
                    current
                        .get("rows")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                } else {
                    next
                };
            }
        }

        let arr = match current.as_array() {
            Some(a) => a,
            None => return String::new(),
        };
        let remaining_path: Vec<&str> = parts[wildcard_pos + 1..].to_vec();
        let values: Vec<String> = arr
            .iter()
            .map(|item| {
                let mut val = item.clone();
                for part in &remaining_path {
                    val = val.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                }
                format!("'{}'", json_value_to_string(&val))
            })
            .collect();
        values.join(",")
    }

    pub fn resolve_deep_path(&self, expr: &str) -> Option<serde_json::Value> {
        if let Some(rest) = expr.strip_prefix("steps.") {
            if let Some(dot_pos) = rest.find('.') {
                let step_id = &rest[..dot_pos];
                let path = &rest[dot_pos + 1..];
                if let Some(step_val) = self.step_results.get(step_id) {
                    let parts: Vec<&str> = path.split('.').collect();
                    let mut current = step_val.clone();
                    for part in &parts {
                        if let Ok(idx) = part.parse::<usize>() {
                            current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                        } else {
                            let next = current
                                .get(*part)
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);
                            current = if next.is_null() && (*part == "data" || *part == "result") {
                                current
                                    .get("rows")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null)
                            } else {
                                next
                            };
                        }
                    }
                    return Some(current);
                }
            }
        }
        None
    }
}

pub fn json_value_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_variables_include_date_and_time() {
        let mut ctx = WorkflowContext::new(&serde_json::json!({}));
        ctx.set_builtin_variables();
        let date = ctx.variables.get("current_date").expect("current_date");
        let time = ctx.variables.get("current_time").expect("current_time");
        let month = ctx.variables.get("current_month").expect("current_month");
        let year = ctx.variables.get("current_year").expect("current_year");

        assert_eq!(date.len(), 10, "YYYY-MM-DD");
        assert_eq!(time.len(), 8, "HH:MM:SS");
        assert_eq!(month.len(), 7, "YYYY-MM");
        assert_eq!(year.len(), 4, "YYYY");
        assert!(date.chars().nth(4) == Some('-') && date.chars().nth(7) == Some('-'));
        assert!(time.chars().nth(2) == Some(':') && time.chars().nth(5) == Some(':'));
    }

    #[test]
    fn typed_values_survive_variables_steps_and_loop_paths() {
        let mut ctx = WorkflowContext::new(&serde_json::json!({
            "limit": 7,
            "name": "O'Brien"
        }));
        assert_eq!(
            ctx.resolve_expression_value("limit").unwrap(),
            serde_json::json!(7)
        );
        assert_eq!(
            ctx.resolve_expression_value("name").unwrap(),
            serde_json::json!("O'Brien")
        );

        ctx.set_step_result("q1", serde_json::json!({ "rows_count": 3 }));
        assert_eq!(
            ctx.resolve_expression_value("steps.q1.rows_count").unwrap(),
            serde_json::json!(3)
        );

        ctx.set_loop_var("item", serde_json::json!({ "id": 9, "name": "Ada" }));
        assert_eq!(
            ctx.resolve_expression_value("item.id").unwrap(),
            serde_json::json!(9)
        );
        assert_eq!(
            ctx.resolve_expression_value("item.name").unwrap(),
            serde_json::json!("Ada")
        );

        ctx.set_loop_var("row", serde_json::json!([{ "id": 11 }]));
        assert_eq!(
            ctx.resolve_expression_value("row[0].id").unwrap(),
            serde_json::json!(11)
        );
        assert!(ctx.resolve_expression_value("missing").is_err());
    }

    #[test]
    fn template_resolves_current_date_and_time() {
        let mut ctx = WorkflowContext::new(&serde_json::json!({}));
        ctx.set_builtin_variables();
        let resolved = ctx
            .resolve_template("d={{current_date}} t={{current_time}}")
            .unwrap();
        assert!(resolved.starts_with("d=20"));
        assert!(resolved.contains(" t="));
        assert!(!resolved.contains("{{"));
    }
}
