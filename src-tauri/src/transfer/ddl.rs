//! Generic DDL builder driven entirely by the IR and a target adapter.

use super::adapter::SyncTargetAdapter;
use super::ir::{IRTable, IRType};

/// Explicit `IRType::Other(native)` from transfer mapping overrides must not be
/// auto-downgraded (e.g. user `VARCHAR(64)` must not become `VARCHAR(16383)`).
fn ddl_ir_type_for_column(
    ir_type: &IRType,
    has_default: bool,
    tgt: &dyn SyncTargetAdapter,
) -> IRType {
    if matches!(ir_type, IRType::Other(_)) {
        return ir_type.clone();
    }
    if has_default && !tgt.allows_column_default(ir_type) {
        return tgt
            .default_capable_type_for(ir_type)
            .unwrap_or_else(|| ir_type.clone());
    }
    ir_type.clone()
}

fn native_type_allows_default(native: &str) -> bool {
    let upper = native.trim().to_ascii_uppercase();
    !(upper.starts_with("TEXT")
        || upper.starts_with("BLOB")
        || upper.starts_with("JSON")
        || upper.starts_with("LONGBLOB")
        || upper.starts_with("MEDIUMBLOB")
        || upper.starts_with("TINYBLOB"))
}

fn column_allows_default(ddl_ir_type: &IRType, tgt: &dyn SyncTargetAdapter) -> bool {
    match ddl_ir_type {
        IRType::Other(native) => native_type_allows_default(native),
        _ => tgt.allows_column_default(ddl_ir_type),
    }
}

/// Build a `CREATE TABLE` statement from an `IRTable` using the target adapter
/// for type rendering, quoting and capability flags.
pub fn build_create_table_ddl(ir_table: &IRTable, tgt: &dyn SyncTargetAdapter) -> String {
    let q = |name: &str| tgt.quote_ident(name);

    let cols: Vec<String> = ir_table
        .columns
        .iter()
        .map(|c| {
            let ddl_ir_type = ddl_ir_type_for_column(&c.ir_type, c.default_expr.is_some(), tgt);
            let mut def = format!("  {} {}", q(&c.name), tgt.ir_type_to_native(&ddl_ir_type));

            if !c.nullable {
                def.push_str(" NOT NULL");
            }

            if c.is_auto_increment {
                if let Some(kw) = tgt.auto_increment_keyword() {
                    def.push_str(&format!(" {kw}"));
                }
            }

            if let Some(ref d) = c.default_expr {
                if column_allows_default(&ddl_ir_type, tgt) {
                    if let Some(s) = tgt.format_default(d) {
                        def.push_str(&format!(" DEFAULT {s}"));
                    }
                }
            }

            if let Some(ref comment) = c.comment {
                if !comment.is_empty() {
                    // Portable SQL comment after column; engines that need COMMENT '…'
                    // syntax should override via adapter-specific DDL later.
                    let escaped = comment.replace("*/", "* /");
                    def.push_str(&format!(" /* {escaped} */"));
                }
            }

            def
        })
        .collect();

    let mut ddl = format!("CREATE TABLE {} (\n{}", q(&ir_table.name), cols.join(",\n"));

    if tgt.supports_primary_key() && !ir_table.primary_keys.is_empty() {
        let pk_cols: Vec<String> = ir_table.primary_keys.iter().map(|k| q(k)).collect();
        ddl.push_str(&format!(",\n  PRIMARY KEY ({})", pk_cols.join(", ")));
    }

    ddl.push_str("\n)");
    if let Some(suffix) = tgt.create_table_suffix(ir_table) {
        ddl.push_str(&format!("\n{suffix}"));
    }
    ddl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Value;
    use crate::transfer::ir::{IRColumn, IRDefault, IRType};

    struct DummyTarget {
        suffix: Option<String>,
    }

    impl DummyTarget {
        fn new() -> Self {
            Self { suffix: None }
        }
    }

    impl SyncTargetAdapter for DummyTarget {
        fn create_table_suffix(&self, ir_table: &IRTable) -> Option<String> {
            self.suffix
                .clone()
                .or_else(|| ir_table.table_options.clone())
        }
        fn ir_type_to_native(&self, ir: &IRType) -> String {
            match ir {
                IRType::Int32 => "INT".into(),
                IRType::Varchar { length: Some(n) } => format!("VARCHAR({n})"),
                IRType::Varchar { length: None } => "TEXT".into(),
                IRType::Other(native) => native.clone(),
                _ => "TEXT".into(),
            }
        }

        fn format_default(&self, d: &IRDefault) -> Option<String> {
            match d {
                IRDefault::CurrentTimestamp => Some("CURRENT_TIMESTAMP".into()),
                IRDefault::Literal(s) => Some(s.clone()),
                IRDefault::RawExpression(_) => None,
            }
        }

        fn allows_column_default(&self, ir_type: &IRType) -> bool {
            !matches!(ir_type, IRType::Varchar { length: None } | IRType::Text)
        }

        fn default_capable_type_for(&self, ir_type: &IRType) -> Option<IRType> {
            match ir_type {
                IRType::Text => Some(IRType::Varchar { length: Some(100) }),
                IRType::Other(_) => Some(IRType::Varchar {
                    length: Some(16_383),
                }),
                _ => None,
            }
        }

        fn format_literal(&self, _v: &Option<Value>, _ir: &IRType) -> String {
            "NULL".into()
        }
    }

    #[test]
    fn basic_ddl_generation() {
        let table = IRTable {
            name: "users".into(),
            columns: vec![
                IRColumn {
                    name: "id".into(),
                    ir_type: IRType::Int32,
                    nullable: false,
                    default_expr: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                    comment: None,
                },
                IRColumn {
                    name: "name".into(),
                    ir_type: IRType::Varchar { length: Some(100) },
                    nullable: true,
                    default_expr: Some(IRDefault::Literal("'anon'".into())),
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                },
            ],
            primary_keys: vec!["id".into()],
            table_options: None,
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(ddl.contains("CREATE TABLE \"users\""));
        assert!(ddl.contains("\"id\" INT NOT NULL"));
        assert!(ddl.contains("\"name\" VARCHAR(100)"));
        assert!(ddl.contains("DEFAULT 'anon'"));
        assert!(ddl.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn ddl_without_primary_key() {
        let table = IRTable {
            name: "logs".into(),
            columns: vec![IRColumn {
                name: "msg".into(),
                ir_type: IRType::Varchar { length: None },
                nullable: true,
                default_expr: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: None,
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(!ddl.contains("PRIMARY KEY"));
    }

    #[test]
    fn ddl_emits_table_suffix() {
        let table = IRTable {
            name: "events".into(),
            columns: vec![IRColumn {
                name: "ts".into(),
                ir_type: IRType::Timestamp {
                    with_timezone: false,
                },
                nullable: false,
                default_expr: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: Some("ENGINE = MergeTree\nORDER BY (ts)".into()),
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(
            ddl.ends_with("ENGINE = MergeTree\nORDER BY (ts)"),
            "ddl={ddl}"
        );
    }

    #[test]
    fn ddl_uses_fallback_type_when_default_on_disallowed_type() {
        let table = IRTable {
            name: "logs".into(),
            columns: vec![IRColumn {
                name: "msg".into(),
                ir_type: IRType::Text,
                nullable: true,
                default_expr: Some(IRDefault::Literal("'hello'".into())),
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: None,
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(ddl.contains("VARCHAR(100)"), "ddl={ddl}");
        assert!(ddl.contains("DEFAULT 'hello'"), "ddl={ddl}");
    }

    #[test]
    fn ddl_emits_column_comment() {
        let table = IRTable {
            name: "t".into(),
            columns: vec![IRColumn {
                name: "note".into(),
                ir_type: IRType::Varchar { length: None },
                nullable: true,
                default_expr: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: Some("user note".into()),
            }],
            primary_keys: vec![],
            table_options: None,
        };
        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(ddl.contains("/* user note */"), "ddl={ddl}");
    }

    #[test]
    fn ddl_honors_explicit_other_native_type_with_default() {
        let table = IRTable {
            name: "products".into(),
            columns: vec![IRColumn {
                name: "name".into(),
                ir_type: IRType::Other("VARCHAR(64)".into()),
                nullable: true,
                default_expr: Some(IRDefault::Literal("'unnamed'".into())),
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            table_options: None,
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget::new());
        assert!(ddl.contains("VARCHAR(64)"), "ddl={ddl}");
        assert!(!ddl.contains("16383"), "ddl={ddl}");
        assert!(ddl.contains("DEFAULT 'unnamed'"), "ddl={ddl}");
    }
}
