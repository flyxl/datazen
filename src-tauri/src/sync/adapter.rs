//! Sync adapter traits — the bridge between native types and the IR.

use super::ir::{IRColumn, IRDefault, IRTable, IRType};
use crate::db::{ColumnSchema, TableSchema, Value};
use std::collections::HashMap;

/// Converts native column metadata into IR (used for the *source* side of a sync).
pub trait SyncSourceAdapter: Send + Sync {
    /// Convert a single column to its IR representation.
    ///
    /// `native_full_type` carries the fully-qualified type string with precision
    /// (e.g. PostgreSQL's `format_type()` output). When `None`, the adapter
    /// falls back to `column.data_type`.
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn;

    /// Convert an entire `TableSchema` to an `IRTable`.
    ///
    /// The default implementation iterates over columns and delegates to
    /// [`column_to_ir`](Self::column_to_ir).
    fn table_to_ir(
        &self,
        schema: &TableSchema,
        full_types: Option<&HashMap<String, String>>,
    ) -> IRTable {
        let pk_set: std::collections::HashSet<&str> =
            schema.primary_keys.iter().map(|s| s.as_str()).collect();

        let columns = schema
            .columns
            .iter()
            .map(|c| {
                let ft = full_types
                    .and_then(|m| m.get(&c.name))
                    .map(|s| s.as_str());
                let mut ir = self.column_to_ir(c, ft);
                if pk_set.contains(c.name.as_str()) {
                    ir.is_primary_key = true;
                }
                ir
            })
            .collect();

        IRTable {
            name: schema.table_name.clone(),
            columns,
            primary_keys: schema.primary_keys.clone(),
        }
    }
}

/// Renders IR back into native DDL fragments (used for the *target* side of a sync).
pub trait SyncTargetAdapter: Send + Sync {
    /// Render an `IRType` as a native DDL type string.
    fn ir_type_to_native(&self, ir_type: &IRType) -> String;

    /// Render an `IRDefault` as the content of a `DEFAULT` clause.
    /// Return `None` to omit the clause entirely (e.g. for auto-increment columns
    /// whose default is handled by the database engine).
    fn format_default(&self, default: &IRDefault) -> Option<String>;

    /// Format a runtime `Value` as a SQL literal suitable for INSERT statements.
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String;

    fn quote_char(&self) -> char {
        '"'
    }

    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    /// Whether the target database supports inline PRIMARY KEY constraints
    /// in CREATE TABLE. OLAP engines typically do not.
    fn supports_primary_key(&self) -> bool {
        true
    }

    /// Keyword appended after the column type for auto-increment columns.
    /// Return `None` if the engine uses a different mechanism (e.g. PG SERIAL/IDENTITY).
    fn auto_increment_keyword(&self) -> Option<&str> {
        None
    }
}
