//! Base ANSI SQL table-template generator — the React
//! `@datazen/driver-sdk` `sqlDialects/baseTableSql.ts` port
//! (`BaseTableSqlGenerator`), carried where consumed (the
//! `lib/sqlDialects` per-dialect modules, T123; `extra.rs`'s T035
//! carries of the update/delete halves stay until their callers rewire
//! to this canonical home).
//!
//! Dialects inherit the generator and override individual statement
//! builders; the Rust port exposes the same four `generate_*` methods
//! plus `generate_sql`'s dispatch, parameterized by the quote char.
//!
//! The schema contract is the crate's [`TableSchema`][crate::relation_metadata::schema::TableSchema]
//! (the landed counterpart of extension-points' `TableSchema`: column
//! name, data type, default value, PK flag, auto-increment flag, and
//! the table's `primaryKeys` list).

use crate::relation_metadata::schema::TableSchema;

use super::strategy::GeneratedSqlType;

/// Base ANSI SQL generator for table template statements.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BaseTableSqlGenerator {
    quote_char: char,
}

impl BaseTableSqlGenerator {
    /// `new BaseTableSqlGenerator(quoteChar)` — React's default is `'"'`;
    /// every landed dialect passes its own char.
    pub(crate) const fn new(quote_char: char) -> Self {
        Self { quote_char }
    }

    /// The constructor's quote char (dialects' overrides read it).
    pub(crate) fn quote_char(&self) -> char {
        self.quote_char
    }

    /// `quote`: backtick and double-quote double themselves inside,
    /// bracket doubles `]`; any other char is a passthrough.
    pub(crate) fn quote(&self, ident: &str) -> String {
        match self.quote_char {
            '`' => format!("`{}`", ident.replace('`', "``")),
            '"' => format!("\"{}\"", ident.replace('"', "\"\"")),
            '[' => format!("[{}]", ident.replace(']', "]]")),
            _ => ident.to_owned(),
        }
    }

    /// `formatTableRef(tableName, schemaPrefix?)`: a non-blank prefix
    /// joins as `<quoted-prefix>.<quoted-table>`.
    pub(crate) fn format_table_ref(&self, table_name: &str, schema_prefix: Option<&str>) -> String {
        if let Some(prefix) = schema_prefix {
            if !prefix.trim().is_empty() {
                return format!("{}.{}", self.quote(prefix.trim()), self.quote(table_name));
            }
        }
        self.quote(table_name)
    }

    /// `generateSelect`: all columns, or `*` with no column metadata.
    pub(crate) fn generate_select(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("SELECT *\nFROM {table_ref};");
        }
        let cols = schema
            .columns
            .iter()
            .map(|c| self.quote(&c.name))
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT {cols}\nFROM {table_ref};")
    }

    /// `generateInsert`: the insertable (non-auto-increment) columns with
    /// placeholder values — the declared default, `0` for int/numeric
    /// types, `''` otherwise; `DEFAULT VALUES` when nothing is insertable.
    pub(crate) fn generate_insert(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("INSERT INTO {table_ref}\nDEFAULT VALUES;");
        }
        let insertable: Vec<_> = schema
            .columns
            .iter()
            .filter(|c| !c.is_auto_increment)
            .collect();
        if insertable.is_empty() {
            return format!("INSERT INTO {table_ref}\nDEFAULT VALUES;");
        }
        let col_list = insertable
            .iter()
            .map(|c| format!("  {}", self.quote(&c.name)))
            .collect::<Vec<_>>()
            .join(",\n");
        let val_list = insertable
            .iter()
            .map(|c| {
                // React's truthiness: an empty-string default falls
                // through to the type-based placeholder.
                if let Some(default_value) = c.default_value.as_deref() {
                    if !default_value.is_empty() {
                        return format!("  {default_value}");
                    }
                }
                let ty = c.data_type.to_lowercase();
                if ty.contains("int") || ty.contains("numeric") {
                    "  0".to_string()
                } else {
                    "  ''".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("INSERT INTO {table_ref} (\n{col_list}\n) VALUES (\n{val_list}\n);")
    }

    /// `generateUpdate`: SET the non-PK columns (all columns when every
    /// column keys the row), WHERE the effective primary keys.
    pub(crate) fn generate_update(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("/* No column metadata available for UPDATE {table_ref} */");
        }
        let non_pk: Vec<_> = schema
            .columns
            .iter()
            .filter(|c| !schema.primary_keys.contains(&c.name) && !c.is_primary_key)
            .collect();
        let target_cols: Vec<&_> = if !non_pk.is_empty() {
            non_pk
        } else {
            schema.columns.iter().collect()
        };
        let set_clauses = target_cols
            .iter()
            .map(|c| format!("  {} = ''", self.quote(&c.name)))
            .collect::<Vec<_>>()
            .join(",\n");

        let pks = effective_pks(schema);
        let where_clause = where_clause(self.quote_char, &pks);
        format!("UPDATE {table_ref}\nSET\n{set_clauses}\nWHERE {where_clause};")
    }

    /// `generateDelete`: WHERE the effective primary keys.
    pub(crate) fn generate_delete(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("/* No column metadata available for DELETE {table_ref} */");
        }
        let pks = effective_pks(schema);
        let where_clause = where_clause(self.quote_char, &pks);
        format!("DELETE FROM {table_ref}\nWHERE {where_clause};")
    }

    /// `generateSql`: dispatch over the statement type.
    pub(crate) fn generate_sql(
        &self,
        kind: GeneratedSqlType,
        table_ref: &str,
        schema: &TableSchema,
    ) -> String {
        match kind {
            GeneratedSqlType::Select => self.generate_select(table_ref, schema),
            GeneratedSqlType::Insert => self.generate_insert(table_ref, schema),
            GeneratedSqlType::Update => self.generate_update(table_ref, schema),
            GeneratedSqlType::Delete => self.generate_delete(table_ref, schema),
        }
    }
}

/// The generator's pk rule: prefer `primaryKeys`, fall back to columns
/// flagged `isPrimaryKey`.
fn effective_pks(schema: &TableSchema) -> Vec<String> {
    if !schema.primary_keys.is_empty() {
        return schema.primary_keys.clone();
    }
    schema
        .columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect()
}

/// The shared WHERE-clause builder of the update/delete generators:
/// `<quoted-pk> = ` per key joined by ` AND ` — the trailing `=` waits
/// for the user's value — or the warning comment with no keys.
fn where_clause(quote_char: char, pks: &[String]) -> String {
    if pks.is_empty() {
        return "/* WARNING: Primary Key not found. Specify condition */".to_owned();
    }
    let quote = |ident: &str| -> String {
        match quote_char {
            '`' => format!("`{}`", ident.replace('`', "``")),
            '"' => format!("\"{}\"", ident.replace('"', "\"\"")),
            '[' => format!("[{}]", ident.replace(']', "]]")),
            _ => ident.to_owned(),
        }
    };
    pks.iter()
        .map(|pk| format!("{} = ", quote(pk)))
        .collect::<Vec<_>>()
        .join(" AND ")
}
