//! Generic DDL builder driven entirely by the IR and a target adapter.

use super::adapter::SyncTargetAdapter;
use super::ir::IRTable;

/// Build a `CREATE TABLE` statement from an `IRTable` using the target adapter
/// for type rendering, quoting and capability flags.
pub fn build_create_table_ddl(ir_table: &IRTable, tgt: &dyn SyncTargetAdapter) -> String {
    let q = |name: &str| tgt.quote_ident(name);

    let cols: Vec<String> = ir_table
        .columns
        .iter()
        .map(|c| {
            let mut def = format!("  {} {}", q(&c.name), tgt.ir_type_to_native(&c.ir_type));

            if !c.nullable {
                def.push_str(" NOT NULL");
            }

            if c.is_auto_increment {
                if let Some(kw) = tgt.auto_increment_keyword() {
                    def.push_str(&format!(" {kw}"));
                }
            }

            if let Some(ref d) = c.default_expr {
                if let Some(s) = tgt.format_default(d) {
                    def.push_str(&format!(" DEFAULT {s}"));
                }
            }

            def
        })
        .collect();

    let mut ddl = format!(
        "CREATE TABLE {} (\n{}",
        q(&ir_table.name),
        cols.join(",\n")
    );

    if tgt.supports_primary_key() && !ir_table.primary_keys.is_empty() {
        let pk_cols: Vec<String> = ir_table.primary_keys.iter().map(|k| q(k)).collect();
        ddl.push_str(&format!(",\n  PRIMARY KEY ({})", pk_cols.join(", ")));
    }

    ddl.push_str("\n)");
    ddl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Value;
    use crate::sync::ir::{IRColumn, IRDefault, IRType};

    struct DummyTarget;

    impl SyncTargetAdapter for DummyTarget {
        fn ir_type_to_native(&self, ir: &IRType) -> String {
            match ir {
                IRType::Int32 => "INT".into(),
                IRType::Varchar { length: Some(n) } => format!("VARCHAR({n})"),
                IRType::Varchar { length: None } => "TEXT".into(),
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
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget);
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
        };

        let ddl = build_create_table_ddl(&table, &DummyTarget);
        assert!(!ddl.contains("PRIMARY KEY"));
    }
}
