//! Workflow execution context and template/path resolution.

use std::collections::HashMap;

pub(crate) struct WorkflowContext {
    pub(crate) variables: HashMap<String, String>,
    step_results: HashMap<String, serde_json::Value>,
    loop_vars: HashMap<String, serde_json::Value>,
    last_step_id: Option<String>,
}

impl WorkflowContext {
    pub(crate) fn new(input: &serde_json::Value) -> Self {
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
            loop_vars: HashMap::new(),
            last_step_id: None,
        }
    }

    pub(crate) fn set_builtin_variables(&mut self) {
        let now = chrono::Local::now();
        self.variables
            .insert("current_month".into(), now.format("%Y-%m").to_string());
        self.variables
            .insert("current_date".into(), now.format("%Y-%m-%d").to_string());
        self.variables
            .insert("current_year".into(), now.format("%Y").to_string());
    }

    pub(crate) fn set_step_result(&mut self, step_id: &str, result: serde_json::Value) {
        self.step_results.insert(step_id.into(), result);
        self.last_step_id = Some(step_id.into());
    }

    pub(crate) fn set_loop_var(&mut self, name: &str, value: serde_json::Value) {
        self.loop_vars.insert(name.into(), value);
    }

    pub(crate) fn clear_loop_var(&mut self, name: &str) {
        self.loop_vars.remove(name);
    }

    pub(crate) fn get_last_result(&self) -> Option<String> {
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
    pub(crate) fn resolve_template(&self, template: &str) -> Result<String, String> {
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}").map_err(|e| e.to_string())?;

        let result = re.replace_all(template, |caps: &regex::Captures| {
            let expr = caps[1].trim();
            self.resolve_expression(expr)
        });

        Ok(result.to_string())
    }

    pub(crate) fn resolve_expression(&self, expr: &str) -> String {
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
        if path.contains(".*") {
            return self.resolve_wildcard_path(value, path);
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
                    let next = current.get(field).cloned().unwrap_or(serde_json::Value::Null);
                    current = if next.is_null() && (field == "data" || field == "result") {
                        current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
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
                let next = current.get(part).cloned().unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (part == "data" || part == "result") {
                    current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
                } else {
                    next
                };
            }
        }

        json_value_to_string(&current)
    }

    fn resolve_wildcard_path(&self, value: &serde_json::Value, path: &str) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        let wildcard_pos = parts.iter().position(|p| *p == "*").unwrap_or(0);

        let mut current = value.clone();
        for part in &parts[..wildcard_pos] {
            if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx).cloned().unwrap_or(serde_json::Value::Null);
            } else {
                let next = current.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                current = if next.is_null() && (*part == "data" || *part == "result") {
                    current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
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

    pub(crate) fn resolve_deep_path(&self, expr: &str) -> Option<serde_json::Value> {
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
                            let next = current.get(*part).cloned().unwrap_or(serde_json::Value::Null);
                            current = if next.is_null() && (*part == "data" || *part == "result") {
                                current.get("rows").cloned().unwrap_or(serde_json::Value::Null)
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

pub(crate) fn json_value_to_string(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}
