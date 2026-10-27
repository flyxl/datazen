//! Parse `.ctx.yaml` table-group context files.
//!
//! Format:
//! ```yaml
//! groups:
//!   - name: "User Management"
//!     tables:
//!       - users
//!       - roles
//!       - permissions
//!   - name: "Order System"
//!     tables:
//!       - orders
//!       - order_items
//! ```
//!
//! When a `.ctx.yaml` file is @-referenced in NL2SQL, the backend extracts the
//! table names and fetches their real-time DDL via `SchemaContextBuilder`.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CtxYamlFile {
    #[serde(default)]
    pub groups: Vec<TableGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableGroup {
    pub name: String,
    pub tables: Vec<String>,
}

impl CtxYamlFile {
    /// Parse a `.ctx.yaml` file from its text content.
    pub fn parse(content: &str) -> Result<Self, String> {
        serde_yaml::from_str(content).map_err(|e| format!("Invalid .ctx.yaml: {e}"))
    }

    /// Collect all unique table names across all groups.
    pub fn all_tables(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut tables = Vec::new();
        for group in &self.groups {
            for table in &group.tables {
                if seen.insert(table.as_str()) {
                    tables.push(table.clone());
                }
            }
        }
        tables
    }
}

/// Check whether a file path has the `.ctx.yaml` extension.
pub fn is_ctx_yaml(path: &str) -> bool {
    path.ends_with(".ctx.yaml") || path.ends_with(".ctx.yml")
}

/// Extract table names from context file entries that are `.ctx.yaml` files.
/// Returns `(ctx_yaml_tables, remaining_entries)`:
/// - `ctx_yaml_tables`: deduplicated table names extracted from all `.ctx.yaml` files
/// - `remaining_entries`: entries that are NOT `.ctx.yaml` (passed through unchanged)
pub fn extract_ctx_yaml_tables(
    entries: &[(String, String)],
) -> (Vec<String>, Vec<(String, String)>) {
    let mut tables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut remaining = Vec::new();

    for (path, content) in entries {
        if is_ctx_yaml(path) {
            match CtxYamlFile::parse(content) {
                Ok(ctx) => {
                    for table in ctx.all_tables() {
                        if seen.insert(table.clone()) {
                            tables.push(table);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "skipping invalid .ctx.yaml");
                    remaining.push((path.clone(), content.clone()));
                }
            }
        } else {
            remaining.push((path.clone(), content.clone()));
        }
    }

    (tables, remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_ctx_yaml() {
        let yaml = r#"
groups:
  - name: "User Management"
    tables:
      - users
      - roles
  - name: "Orders"
    tables:
      - orders
      - order_items
"#;
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert_eq!(ctx.groups.len(), 2);
        assert_eq!(ctx.groups[0].name, "User Management");
        assert_eq!(ctx.groups[0].tables, vec!["users", "roles"]);
        assert_eq!(ctx.groups[1].tables, vec!["orders", "order_items"]);
    }

    #[test]
    fn all_tables_deduplicates() {
        let yaml = r#"
groups:
  - name: A
    tables: [users, orders]
  - name: B
    tables: [orders, products]
"#;
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        let tables = ctx.all_tables();
        assert_eq!(tables, vec!["users", "orders", "products"]);
    }

    #[test]
    fn parse_empty_groups() {
        let yaml = "groups: []";
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert!(ctx.groups.is_empty());
        assert!(ctx.all_tables().is_empty());
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let yaml = "not: [valid: yaml: {";
        assert!(CtxYamlFile::parse(yaml).is_err());
    }

    #[test]
    fn is_ctx_yaml_checks_extension() {
        assert!(is_ctx_yaml("tables.ctx.yaml"));
        assert!(is_ctx_yaml("path/to/sales.ctx.yml"));
        assert!(!is_ctx_yaml("notes.yaml"));
        assert!(!is_ctx_yaml("schema.sql"));
        assert!(!is_ctx_yaml("ctx.yaml.bak"));
    }

    #[test]
    fn extract_separates_ctx_yaml_from_regular() {
        let entries = vec![
            (
                "tables.ctx.yaml".into(),
                "groups:\n  - name: A\n    tables: [users, orders]".into(),
            ),
            ("notes.md".into(), "# Some notes".into()),
            (
                "more.ctx.yaml".into(),
                "groups:\n  - name: B\n    tables: [orders, products]".into(),
            ),
        ];
        let (tables, remaining) = extract_ctx_yaml_tables(&entries);
        assert_eq!(tables, vec!["users", "orders", "products"]);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0, "notes.md");
    }

    #[test]
    fn extract_handles_invalid_ctx_yaml_gracefully() {
        let entries = vec![
            ("bad.ctx.yaml".into(), "not: [valid:".into()),
            ("good.md".into(), "hello".into()),
        ];
        let (tables, remaining) = extract_ctx_yaml_tables(&entries);
        assert!(tables.is_empty());
        assert_eq!(remaining.len(), 2);
    }

    #[test]
    fn parse_single_group() {
        let yaml = r#"
groups:
  - name: Core
    tables: [users]
"#;
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert_eq!(ctx.groups.len(), 1);
        assert_eq!(ctx.all_tables(), vec!["users"]);
    }

    #[test]
    fn parse_no_groups_key_defaults_to_empty() {
        let yaml = "other_key: value";
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert!(ctx.groups.is_empty());
    }

    #[test]
    fn extract_deduplicates_across_multiple_ctx_files() {
        let entries = vec![
            (
                "a.ctx.yaml".into(),
                "groups:\n  - name: A\n    tables: [users, orders]".into(),
            ),
            (
                "b.ctx.yaml".into(),
                "groups:\n  - name: B\n    tables: [orders, products, users]".into(),
            ),
        ];
        let (tables, remaining) = extract_ctx_yaml_tables(&entries);
        assert_eq!(tables, vec!["users", "orders", "products"]);
        assert!(remaining.is_empty());
    }

    #[test]
    fn extract_mixed_ctx_yaml_and_regular_files() {
        let entries = vec![
            ("schema.sql".into(), "CREATE TABLE users ...".into()),
            (
                "tables.ctx.yaml".into(),
                "groups:\n  - name: A\n    tables: [users]".into(),
            ),
            ("notes.md".into(), "# Notes".into()),
        ];
        let (tables, remaining) = extract_ctx_yaml_tables(&entries);
        assert_eq!(tables, vec!["users"]);
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].0, "schema.sql");
        assert_eq!(remaining[1].0, "notes.md");
    }

    #[test]
    fn is_ctx_yaml_handles_edge_cases() {
        assert!(!is_ctx_yaml(""));
        assert!(!is_ctx_yaml(".ctx.yaml.bak"));
        assert!(is_ctx_yaml("a.ctx.yaml"));
        assert!(is_ctx_yaml("dir/sub/tables.ctx.yml"));
        assert!(!is_ctx_yaml("ctx.yaml"));
        assert!(!is_ctx_yaml("file.yaml"));
    }

    #[test]
    fn all_tables_preserves_order() {
        let yaml = r#"
groups:
  - name: Z
    tables: [zebra, apple]
  - name: A
    tables: [banana]
"#;
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert_eq!(ctx.all_tables(), vec!["zebra", "apple", "banana"]);
    }

    #[test]
    fn parse_group_with_empty_tables() {
        let yaml = r#"
groups:
  - name: Empty
    tables: []
  - name: Some
    tables: [users]
"#;
        let ctx = CtxYamlFile::parse(yaml).unwrap();
        assert_eq!(ctx.groups.len(), 2);
        assert_eq!(ctx.all_tables(), vec!["users"]);
    }
}
