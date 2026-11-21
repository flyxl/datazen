mod common;
#[cfg(test)]
mod tests;

pub use common::{COMMON_FUNCTION_NAMES, common_functions};

use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: String,
    pub param_type: String,
    pub optional: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionEntry {
    pub name: String,
    pub description: String,
    pub params: Vec<FunctionParam>,
    pub dialects: Option<Vec<String>>,
    pub returns_value: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DriverFunctionMeta {
    pub sql_dialect: Option<String>,
    pub sql_functions: Vec<FunctionEntry>,
}

pub type DriverFunctionRegistry = Vec<(String, DriverFunctionMeta)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCompletion {
    pub label: String,
    pub completion_type: String,
    pub detail: String,
    pub apply: String,
}

pub const DIALECT_ALIASES: [(&str, &str); 6] = [
    ("postgres", "postgresql"),
    ("pg", "postgresql"),
    ("cockroach", "postgresql"),
    ("mariadb", "mysql"),
    ("tidb", "mysql"),
    ("mssql", "sqlserver"),
];

pub fn resolve_dialect_family(dialect_id: &str) -> String {
    let lower = dialect_id.to_lowercase();
    DIALECT_ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map(|(_, canonical)| (*canonical).to_owned())
        .unwrap_or(lower)
}

pub fn get_driver_functions(
    dialect_id: Option<&str>,
    registry: &DriverFunctionRegistry,
) -> Vec<FunctionEntry> {
    match dialect_id {
        None => {
            let mut functions = Vec::new();
            let mut seen = BTreeSet::new();
            for (_, meta) in registry {
                for function in &meta.sql_functions {
                    let key = format!("{}::{}", function.name.to_uppercase(), function.params.len());
                    if seen.insert(key) {
                        functions.push(function.clone());
                    }
                }
            }
            functions
        }
        Some(id) => {
            let normalized = id.to_lowercase();
            let canonical = resolve_dialect_family(id);
            let direct = registry
                .iter()
                .find(|(key, _)| key == &canonical)
                .or_else(|| registry.iter().find(|(key, _)| key == &normalized));
            if let Some((_, meta)) = direct {
                if !meta.sql_functions.is_empty() {
                    return meta.sql_functions.clone();
                }
            }
            for (key, meta) in registry {
                let dialect_match = meta
                    .sql_dialect
                    .as_deref()
                    .map(|dialect| dialect.to_lowercase() == canonical)
                    .unwrap_or(false);
                if (dialect_match || key.to_lowercase() == canonical)
                    && !meta.sql_functions.is_empty()
                {
                    return meta.sql_functions.clone();
                }
            }
            Vec::new()
        }
    }
}

pub fn get_function_entries(
    dialect_id: Option<&str>,
    registry: &DriverFunctionRegistry,
) -> Vec<FunctionEntry> {
    let mut entries = common_functions().to_vec();
    entries.extend(get_driver_functions(dialect_id, registry));
    entries
}

pub fn build_function_completions(
    dialect_id: Option<&str>,
    registry: &DriverFunctionRegistry,
) -> Vec<FunctionCompletion> {
    get_function_entries(dialect_id, registry)
        .into_iter()
        .map(|entry| FunctionCompletion {
            label: entry.name.clone(),
            completion_type: "function".to_owned(),
            detail: entry.description,
            apply: format!("{}(", entry.name),
        })
        .collect()
}

pub fn to_sql_function_completions(
    entries: Vec<FunctionCompletion>,
) -> Vec<crate::SqlFunctionCompletion> {
    entries
        .into_iter()
        .map(|entry| crate::SqlFunctionCompletion {
            label: entry.label,
            completion_type: entry.completion_type,
            apply: entry.apply,
        })
        .collect()
}
