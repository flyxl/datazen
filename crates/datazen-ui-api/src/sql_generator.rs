use crate::types::ColumnSchema;
use crate::TableSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedSqlType {
    Select,
    Insert,
    Update,
    Delete,
}

impl GeneratedSqlType {
    pub fn as_str(self) -> &'static str {
        match self {
            GeneratedSqlType::Select => "select",
            GeneratedSqlType::Insert => "insert",
            GeneratedSqlType::Update => "update",
            GeneratedSqlType::Delete => "delete",
        }
    }
}

impl std::fmt::Display for GeneratedSqlType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct TableSqlDialect {
    pub format_table_ref: Box<dyn Fn(&str, Option<&str>) -> String>,
    pub generate_sql: Box<dyn Fn(GeneratedSqlType, &str, &TableSchema) -> String>,
}

pub struct BaseTableSqlGenerator {
    pub quote_char: String,
}

impl BaseTableSqlGenerator {
    pub fn new(quote_char: &str) -> Self {
        Self { quote_char: quote_char.to_owned() }
    }

    pub fn quote(&self, ident: &str) -> String {
        match self.quote_char.as_str() {
            "`" => format!("`{}`", ident.replace('`', "``")),
            "\"" => format!("\"{}\"", ident.replace('"', "\"\"")),
            "[" => format!("[{}]", ident.replace(']', "]]")),
            _ => ident.to_owned(),
        }
    }

    pub fn format_table_ref(&self, table_name: &str, schema_prefix: Option<&str>) -> String {
        match schema_prefix {
            Some(prefix) if !prefix.trim().is_empty() => {
                format!("{}.{}", self.quote(prefix.trim()), self.quote(table_name))
            }
            _ => self.quote(table_name),
        }
    }

    pub fn generate_select(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("SELECT *\nFROM {table_ref};");
        }
        let cols = schema.columns.iter().map(|column| self.quote(&column.name)).collect::<Vec<_>>().join(", ");
        format!("SELECT {cols}\nFROM {table_ref};")
    }

    pub fn generate_insert(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("INSERT INTO {table_ref}\nDEFAULT VALUES;");
        }
        let insertable: Vec<&ColumnSchema> = schema
            .columns
            .iter()
            .filter(|column| column.is_auto_increment != Some(true))
            .collect();
        if insertable.is_empty() {
            return format!("INSERT INTO {table_ref}\nDEFAULT VALUES;");
        }
        let col_list = insertable.iter().map(|column| format!("  {}", self.quote(&column.name))).collect::<Vec<_>>().join(",\n");
        let val_list = insertable
            .iter()
            .map(|column| {
                if let Some(default) = column.default_value.as_ref().filter(|value| !value.is_empty()) {
                    return format!("  {default}");
                }
                let lower = column.data_type.to_lowercase();
                if lower.contains("int") || lower.contains("numeric") {
                    "  0".to_owned()
                } else {
                    "  ''".to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!("INSERT INTO {table_ref} (\n{col_list}\n) VALUES (\n{val_list}\n);")
    }

    fn primary_key_names(&self, schema: &TableSchema) -> Vec<String> {
        if !schema.primary_keys.is_empty() {
            return schema.primary_keys.clone();
        }
        schema
            .columns
            .iter()
            .filter(|column| column.is_primary_key == Some(true))
            .map(|column| column.name.clone())
            .collect()
    }

    fn where_clause(&self, schema: &TableSchema) -> String {
        let pks = self.primary_key_names(schema);
        if pks.is_empty() {
            return "/* WARNING: Primary Key not found. Specify condition */".to_owned();
        }
        pks.iter().map(|pk| format!("{} = ", self.quote(pk))).collect::<Vec<_>>().join(" AND ")
    }

    pub fn generate_update(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("/* No column metadata available for UPDATE {table_ref} */");
        }
        let non_pk: Vec<&ColumnSchema> = schema
            .columns
            .iter()
            .filter(|column| {
                !schema.primary_keys.contains(&column.name) && column.is_primary_key != Some(true)
            })
            .collect();
        let targets: Vec<&ColumnSchema> = if non_pk.is_empty() { schema.columns.iter().collect() } else { non_pk };
        let set_clauses = targets
            .iter()
            .map(|column| format!("  {} = ''", self.quote(&column.name)))
            .collect::<Vec<_>>()
            .join(",\n");
        format!("UPDATE {table_ref}\nSET\n{set_clauses}\nWHERE {};", self.where_clause(schema))
    }

    pub fn generate_delete(&self, table_ref: &str, schema: &TableSchema) -> String {
        if schema.columns.is_empty() {
            return format!("/* No column metadata available for DELETE {table_ref} */");
        }
        format!("DELETE FROM {table_ref}\nWHERE {};", self.where_clause(schema))
    }

    pub fn generate_sql(&self, sql_type: GeneratedSqlType, table_ref: &str, schema: &TableSchema) -> String {
        match sql_type {
            GeneratedSqlType::Select => self.generate_select(table_ref, schema),
            GeneratedSqlType::Insert => self.generate_insert(table_ref, schema),
            GeneratedSqlType::Update => self.generate_update(table_ref, schema),
            GeneratedSqlType::Delete => self.generate_delete(table_ref, schema),
        }
    }
}

pub fn format_table_identifier(
    table_name: &str,
    schema_prefix: Option<&str>,
    dialect: Option<&TableSqlDialect>,
    fallback_quote: &str,
) -> String {
    if let Some(dialect) = dialect {
        return (dialect.format_table_ref)(table_name, schema_prefix);
    }
    BaseTableSqlGenerator::new(fallback_quote).format_table_ref(table_name, schema_prefix)
}

pub fn generate_table_sql(
    schema: &TableSchema,
    sql_type: GeneratedSqlType,
    schema_prefix: Option<&str>,
    dialect: Option<&TableSqlDialect>,
    fallback_quote: &str,
) -> String {
    let base = BaseTableSqlGenerator::new(fallback_quote);
    let table_ref = match dialect {
        Some(dialect) => (dialect.format_table_ref)(&schema.table_name, schema_prefix),
        None => base.format_table_ref(&schema.table_name, schema_prefix),
    };
    match dialect {
        Some(dialect) => (dialect.generate_sql)(sql_type, &table_ref, schema),
        None => base.generate_sql(sql_type, &table_ref, schema),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexInfo;

    fn column(name: &str, data_type: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            nullable: true,
            default_value: None,
            is_primary_key: None,
            is_auto_increment: None,
            comment: None,
        }
    }

    fn schema(columns: Vec<ColumnSchema>, primary_keys: Vec<String>) -> TableSchema {
        TableSchema {
            table_name: "users".to_owned(),
            columns,
            primary_keys,
            indexes: Vec::<IndexInfo>::new(),
            foreign_keys: Vec::new(),
        }
    }

    #[test]
    fn generated_types_keep_their_literals() {
        assert_eq!(GeneratedSqlType::Select.as_str(), "select");
        assert_eq!(GeneratedSqlType::Insert.as_str(), "insert");
        assert_eq!(GeneratedSqlType::Update.as_str(), "update");
        assert_eq!(GeneratedSqlType::Delete.as_str(), "delete");
    }

    #[test]
    fn quoting_follows_the_configured_character() {
        assert_eq!(BaseTableSqlGenerator::new("\"").quote("a\"b"), "\"a\"\"b\"");
        assert_eq!(BaseTableSqlGenerator::new("`").quote("a`b"), "`a``b`");
        assert_eq!(BaseTableSqlGenerator::new("[").quote("a]b"), "[a]]b]");
        assert_eq!(BaseTableSqlGenerator::new("").quote("a"), "a");
    }

    #[test]
    fn table_ref_uses_schema_prefix_only_when_nonblank() {
        let base = BaseTableSqlGenerator::new("\"");
        assert_eq!(base.format_table_ref("users", None), "\"users\"");
        assert_eq!(base.format_table_ref("users", Some("  ")), "\"users\"");
        assert_eq!(base.format_table_ref("users", Some(" pub ")), "\"pub\".\"users\"");
    }

    #[test]
    fn select_and_insert_templates() {
        let base = BaseTableSqlGenerator::new("\"");
        let empty = schema(Vec::new(), Vec::new());
        assert_eq!(base.generate_sql(GeneratedSqlType::Select, "\"users\"", &empty), "SELECT *\nFROM \"users\";");
        assert_eq!(base.generate_sql(GeneratedSqlType::Insert, "\"users\"", &empty), "INSERT INTO \"users\"\nDEFAULT VALUES;");

        let full = schema(vec![column("id", "int"), column("name", "varchar")], vec!["id".to_owned()]);
        assert_eq!(base.generate_sql(GeneratedSqlType::Select, "\"users\"", &full), "SELECT \"id\", \"name\"\nFROM \"users\";");
        assert_eq!(
            base.generate_sql(GeneratedSqlType::Insert, "\"users\"", &full),
            "INSERT INTO \"users\" (\n  \"id\",\n  \"name\"\n) VALUES (\n  0,\n  ''\n);"
        );
    }

    #[test]
    fn insert_respects_defaults_and_skips_auto_increment() {
        let base = BaseTableSqlGenerator::new("\"");
        let mut id = column("id", "BIGINT");
        id.is_auto_increment = Some(true);
        let mut created = column("created_at", "timestamp");
        created.default_value = Some("CURRENT_TIMESTAMP".to_owned());
        let full = schema(vec![id, created, column("count", "numeric(10)")], Vec::new());
        assert_eq!(
            base.generate_insert("t", &full),
            "INSERT INTO t (\n  \"created_at\",\n  \"count\"\n) VALUES (\n  CURRENT_TIMESTAMP,\n  0\n);"
        );
        let only_auto = schema(vec![{
            let mut c = column("id", "int");
            c.is_auto_increment = Some(true);
            c
        }], Vec::new());
        assert_eq!(base.generate_insert("t", &only_auto), "INSERT INTO t\nDEFAULT VALUES;");
    }

    #[test]
    fn update_and_delete_use_primary_keys_or_warn() {
        let base = BaseTableSqlGenerator::new("\"");
        let full = schema(vec![column("id", "int"), column("name", "text")], vec!["id".to_owned()]);
        assert_eq!(
            base.generate_sql(GeneratedSqlType::Update, "t", &full),
            "UPDATE t\nSET\n  \"name\" = ''\nWHERE \"id\" = ;"
        );
        assert_eq!(base.generate_sql(GeneratedSqlType::Delete, "t", &full), "DELETE FROM t\nWHERE \"id\" = ;");
        let no_pk = schema(vec![column("name", "text")], Vec::new());
        assert_eq!(
            base.generate_sql(GeneratedSqlType::Delete, "t", &no_pk),
            "DELETE FROM t\nWHERE /* WARNING: Primary Key not found. Specify condition */;"
        );
        let empty = schema(Vec::new(), Vec::new());
        assert_eq!(base.generate_update("t", &empty), "/* No column metadata available for UPDATE t */");
        assert_eq!(base.generate_delete("t", &empty), "/* No column metadata available for DELETE t */");
    }

    #[test]
    fn identifier_and_generation_use_the_injected_dialect() {
        let dialect = TableSqlDialect {
            format_table_ref: Box::new(|table, schema| match schema {
                Some(schema) => format!("db.{schema}.{table}"),
                None => format!("db.{table}"),
            }),
            generate_sql: Box::new(|kind, table_ref, _| format!("{kind} {table_ref}")),
        };
        assert_eq!(format_table_identifier("users", Some("pub"), Some(&dialect), "\""), "db.pub.users");
        assert_eq!(format_table_identifier("users", None, Some(&dialect), "\""), "db.users");
        let result = generate_table_sql(&schema(Vec::new(), Vec::new()), GeneratedSqlType::Select, Some("pub"), Some(&dialect), "\"");
        assert_eq!(result, "select db.pub.users");

        assert_eq!(format_table_identifier("users", Some("pub"), None, "`"), "`pub`.`users`");
        assert_eq!(
            generate_table_sql(&schema(Vec::new(), Vec::new()), GeneratedSqlType::Select, None, None, "\""),
            "SELECT *\nFROM \"users\";"
        );
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::format_table_identifier;
        let _ = crate::generate_table_sql;
        let _: Option<crate::GeneratedSqlType> = None;
        let _: Option<crate::BaseTableSqlGenerator> = None;
        let _: Option<crate::TableSqlDialect> = None;
        assert_eq!(crate::GeneratedSqlType::Delete.as_str(), "delete");
    }
}
