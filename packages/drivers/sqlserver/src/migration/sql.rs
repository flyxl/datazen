use super::validation::*;
use super::*;
use std::collections::HashSet;

pub(super) fn default_constraint_lookup(table: &str, column: &str) -> Result<String, String> {
    let parts = relation_parts(table)?;
    let qualified_table = relation(table)?;
    let catalog = relation_catalog_prefix(&parts);
    let object_literal = sql_string(&qualified_table);
    let column_literal = sql_string(column);
    Ok(format!(
        "DECLARE @datazen_default_name sysname; DECLARE @datazen_default_definition nvarchar(max); \
         SELECT @datazen_default_name = dc.name, @datazen_default_definition = dc.definition \
         FROM {catalog}sys.default_constraints AS dc \
         JOIN {catalog}sys.columns AS c ON c.object_id = dc.parent_object_id AND c.column_id = dc.parent_column_id \
         WHERE dc.parent_object_id = OBJECT_ID({object_literal}, N'U') AND c.name = {column_literal};"
    ))
}

pub(super) fn set_default_sql(
    table: &str,
    column: &str,
    expected_from: Option<&str>,
    desired_to: Option<&str>,
) -> Result<String, String> {
    let table_sql = relation(table)?;
    let column_sql = quote_ident(column)?;
    let mut sql = default_constraint_lookup(table, column)?;
    match expected_from {
        Some(expected) => {
            let expected = literal_expression(expected, "expected DEFAULT expression")?;
            sql.push_str(&format!(
                " IF @datazen_default_name IS NULL OR @datazen_default_definition <> {} \
                 THROW 51000, N'SQL Server DEFAULT changed after schema comparison', 1;",
                sql_string(&expected)
            ));
        }
        None => sql.push_str(
            " IF @datazen_default_name IS NOT NULL THROW 51000, N'SQL Server DEFAULT appeared after schema comparison', 1;",
        ),
    }
    sql.push_str(&format!(
        " IF @datazen_default_name IS NOT NULL EXEC(N'ALTER TABLE {table_sql} DROP CONSTRAINT ' + QUOTENAME(@datazen_default_name));"
    ));
    if let Some(default) = desired_to {
        let default = literal_expression(default, "DEFAULT expression")?;
        sql.push_str(&format!(
            " ALTER TABLE {table_sql} ADD DEFAULT ({default}) FOR {column_sql};"
        ));
    }
    Ok(sql)
}

pub(super) fn drop_default_for_column_sql(
    column: &MigrationColumn,
    table: &str,
) -> Result<String, String> {
    let table_sql = relation(table)?;
    let mut sql = default_constraint_lookup(table, &column.name)?;
    match column.default_value.as_deref() {
        Some(expected) => {
            let expected = literal_expression(expected, "expected DEFAULT expression")?;
            sql.push_str(&format!(
                " IF @datazen_default_name IS NULL OR @datazen_default_definition <> {} \
                 THROW 51000, N'SQL Server DEFAULT changed after schema comparison', 1;",
                sql_string(&expected)
            ));
        }
        None => sql.push_str(
            " IF @datazen_default_name IS NOT NULL THROW 51000, N'SQL Server DEFAULT appeared after schema comparison', 1;",
        ),
    }
    sql.push_str(&format!(
        " IF @datazen_default_name IS NOT NULL EXEC(N'ALTER TABLE {table_sql} DROP CONSTRAINT ' + QUOTENAME(@datazen_default_name));"
    ));
    Ok(sql)
}

pub(super) fn is_character_type(data_type: &str) -> bool {
    let (base, _, _) = parse_type_parts(data_type);
    matches!(base.as_str(), "CHAR" | "VARCHAR" | "NCHAR" | "NVARCHAR")
}

pub(super) fn alter_column_type_sql(table: &str, column: &str, to: &str) -> Result<String, String> {
    let parts = relation_parts(table)?;
    let catalog = relation_catalog_prefix(&parts);
    let table_sql = relation(table)?;
    let object_literal = sql_string(&table_sql);
    let column_literal = sql_string(column);
    let column_sql = quote_ident(column)?;
    let mut rendered_type = sql_string(to);
    if is_character_type(to) {
        rendered_type = format!(
            "{rendered_type} + CASE WHEN @datazen_collation IS NULL THEN N'' ELSE N' COLLATE ' + QUOTENAME(@datazen_collation) END"
        );
    }
    Ok(format!(
        "DECLARE @datazen_nullability nvarchar(8); DECLARE @datazen_collation sysname; \
         SELECT @datazen_nullability = CASE WHEN c.is_nullable = 1 THEN N' NULL' ELSE N' NOT NULL' END, \
                @datazen_collation = c.collation_name \
         FROM {catalog}sys.columns AS c \
         WHERE c.object_id = OBJECT_ID({object_literal}, N'U') AND c.name = {column_literal}; \
         IF @datazen_nullability IS NULL THROW 51001, N'SQL Server column changed after schema comparison', 1; \
         EXEC(N'ALTER TABLE {table_sql} ALTER COLUMN {column_sql} ' + {rendered_type} + @datazen_nullability);"
    ))
}

pub(super) fn catalog_type_sql(
    table: &str,
    column: &str,
    nullable: bool,
) -> Result<String, String> {
    let parts = relation_parts(table)?;
    let catalog = relation_catalog_prefix(&parts);
    let table_sql = relation(table)?;
    let object_literal = sql_string(&table_sql);
    let column_literal = sql_string(column);
    let nullability = if nullable { "NULL" } else { "NOT NULL" };
    let generated_type = format!(
        "CASE WHEN tp.is_user_defined = 1 THEN QUOTENAME(ts.name) + N'.' + QUOTENAME(tp.name) \
         WHEN tp.name IN ('nvarchar','nchar') THEN tp.name + N'(' + CASE WHEN c.max_length = -1 THEN N'max' ELSE CONVERT(nvarchar(10), c.max_length / 2) END + N')' \
         WHEN tp.name IN ('varchar','char','varbinary','binary') THEN tp.name + N'(' + CASE WHEN c.max_length = -1 THEN N'max' ELSE CONVERT(nvarchar(10), c.max_length) END + N')' \
         WHEN tp.name IN ('decimal','numeric') THEN tp.name + N'(' + CONVERT(nvarchar(10), c.precision) + N',' + CONVERT(nvarchar(10), c.scale) + N')' \
         WHEN tp.name IN ('time','datetime2','datetimeoffset') THEN tp.name + N'(' + CONVERT(nvarchar(10), c.scale) + N')' \
         WHEN tp.name = 'float' THEN tp.name + N'(' + CONVERT(nvarchar(10), c.precision) + N')' \
         ELSE tp.name END"
    );
    let column_sql = quote_ident(column)?;
    Ok(format!(
        "DECLARE @datazen_type nvarchar(512); DECLARE @datazen_collation sysname; DECLARE @datazen_unsupported bit; \
         SELECT @datazen_type = {generated_type}, @datazen_collation = c.collation_name, \
                @datazen_unsupported = CASE WHEN tp.is_user_defined = 1 OR tp.name IN ('xml','sql_variant','geography','geometry','hierarchyid') THEN 1 ELSE 0 END \
         FROM {catalog}sys.columns AS c \
         JOIN {catalog}sys.types AS tp ON tp.user_type_id = c.user_type_id \
         JOIN {catalog}sys.schemas AS ts ON ts.schema_id = tp.schema_id \
         WHERE c.object_id = OBJECT_ID({object_literal}, N'U') AND c.name = {column_literal}; \
         IF @datazen_type IS NULL THROW 51001, N'SQL Server column changed after schema comparison', 1; \
         IF @datazen_unsupported = 1 THROW 51003, N'SQL Server column type is outside the safe nullability migration subset', 1; \
         EXEC(N'ALTER TABLE {table_sql} ALTER COLUMN {column_sql} ' + @datazen_type + \
              CASE WHEN @datazen_collation IS NULL THEN N'' ELSE N' COLLATE ' + QUOTENAME(@datazen_collation) END + \
              N' {nullability}');"
    ))
}

pub(super) fn primary_key_lookup(table: &str, columns: &[String]) -> Result<String, String> {
    validate_unique_columns(columns, "primary key")?;
    let parts = relation_parts(table)?;
    let catalog = relation_catalog_prefix(&parts);
    let table_sql = relation(table)?;
    let object_literal = sql_string(&table_sql);
    let count = columns.len();
    let matches = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            Ok(format!(
                "(ic.key_ordinal = {} AND COL_NAME(ic.object_id, ic.column_id) = {})",
                index + 1,
                sql_string(column)
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(" OR ");
    Ok(format!(
        "DECLARE @datazen_pk_name sysname; DECLARE @datazen_pk_count int; DECLARE @datazen_pk_matches int; \
         SELECT @datazen_pk_name = kc.name, @datazen_pk_count = COUNT(*), \
         @datazen_pk_matches = SUM(CASE WHEN {matches} THEN 1 ELSE 0 END) \
         FROM {catalog}sys.key_constraints AS kc \
         JOIN {catalog}sys.index_columns AS ic ON ic.object_id = kc.parent_object_id AND ic.index_id = kc.unique_index_id \
         WHERE kc.parent_object_id = OBJECT_ID({object_literal}, N'U') AND kc.type = 'PK' AND ic.key_ordinal > 0 \
         GROUP BY kc.name; \
         IF @datazen_pk_name IS NULL OR @datazen_pk_count <> {count} OR @datazen_pk_matches <> {count} \
         THROW 51002, N'SQL Server primary key changed after schema comparison', 1;"
    ))
}

pub(super) fn comment_property_sql(
    table: &str,
    column: &str,
    value: Option<&str>,
) -> Result<String, String> {
    let parts = relation_parts(table)?;
    if parts.len() < 2 {
        return Err("SQL Server column comments require an explicit schema and table".into());
    }
    let schema = parts[parts.len() - 2].as_str();
    let table_name = parts[parts.len() - 1].as_str();
    let catalog = relation_catalog_prefix(&parts);
    let procedure_prefix = catalog.trim_end_matches('.');
    let procedure_prefix = if procedure_prefix.is_empty() {
        String::new()
    } else {
        format!("{procedure_prefix}.")
    };
    let object_literal = sql_string(&relation(table)?);
    let column_literal = sql_string(column);
    let property_exists = format!(
        "EXISTS (SELECT 1 FROM {catalog}sys.extended_properties AS ep \
         JOIN {catalog}sys.columns AS c ON c.object_id = ep.major_id AND c.column_id = ep.minor_id \
         WHERE ep.class = 1 AND ep.name = N'MS_Description' \
           AND ep.major_id = OBJECT_ID({object_literal}, N'U') AND c.name = {column_literal})"
    );
    let levels = format!(
        "@name=N'MS_Description', @level0type=N'SCHEMA', @level0name={}, \
         @level1type=N'TABLE', @level1name={}, @level2type=N'COLUMN', @level2name={}",
        sql_string(schema),
        sql_string(table_name),
        sql_string(column)
    );
    match value {
        Some(value) => {
            if value
                .chars()
                .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
            {
                return Err("SQL Server column comment contains control characters".into());
            }
            let value = sql_string(value);
            Ok(format!(
                "IF {property_exists} EXEC {procedure_prefix}sys.sp_updateextendedproperty {levels}, @value={value}; \
                 ELSE EXEC {procedure_prefix}sys.sp_addextendedproperty {levels}, @value={value};"
            ))
        }
        None => Ok(format!(
            "IF {property_exists} EXEC {procedure_prefix}sys.sp_dropextendedproperty {levels};"
        )),
    }
}

pub(super) fn create_table_sql(
    table: &str,
    columns: &[MigrationColumn],
    primary_keys: &[String],
) -> Result<MigrationStatement, String> {
    if columns.is_empty() {
        return Err("SQL Server CREATE TABLE requires at least one column".into());
    }
    let table = relation(table)?;
    let mut seen = HashSet::new();
    let mut definitions = Vec::with_capacity(columns.len() + 1);
    let mut identity_count = 0;
    for column in columns {
        if !seen.insert(column.name.as_str()) {
            return Err(format!(
                "SQL Server CREATE TABLE repeats column '{}'",
                column.name
            ));
        }
        identity_count += usize::from(column.is_auto_increment);
        definitions.push(validate_column(column)?);
    }
    if identity_count > 1 {
        return Err("SQL Server permits only one IDENTITY column per table".into());
    }
    if !primary_keys.is_empty() {
        validate_unique_columns(primary_keys, "primary key")?;
        if primary_keys
            .iter()
            .any(|column| !seen.contains(column.as_str()))
        {
            return Err(
                "SQL Server primary key references a column absent from CREATE TABLE".into(),
            );
        }
        definitions.push(format!(
            "PRIMARY KEY ({})",
            primary_keys
                .iter()
                .map(|column| quote_ident(column))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ));
    }
    let sql = format!("CREATE TABLE {table} ({})", definitions.join(", "));
    let mut full_sql = sql;
    for column in columns {
        if let Some(comment) = column.comment.as_deref() {
            full_sql.push_str("; ");
            full_sql.push_str(&comment_property_sql(&table, &column.name, Some(comment))?);
        }
    }
    Ok(MigrationStatement {
        sql: full_sql,
        risk: MigrationRisk::Additive,
        rollback_sql: Some(format!("DROP TABLE {table}")),
        summary: format!("CREATE TABLE {table}"),
    })
}

pub(super) fn render_index_create(
    table: &str,
    index: &IndexInfo,
) -> Result<MigrationStatement, String> {
    validate_unique_columns(&index.columns, "index")?;
    let table = relation(table)?;
    let name = quote_ident(&index.name)?;
    let columns = index
        .columns
        .iter()
        .map(|column| quote_ident(column))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let (is_constraint, index_type) = index
        .index_type
        .strip_prefix("UNIQUE_CONSTRAINT:")
        .map(|kind| (true, kind))
        .unwrap_or((false, index.index_type.as_str()));
    let method = match index_type.to_ascii_uppercase().as_str() {
        "CLUSTERED" => "CLUSTERED",
        "NONCLUSTERED" => "NONCLUSTERED",
        other => return Err(format!("unsupported SQL Server index type '{other}'")),
    };
    let (sql, rollback_sql) = if is_constraint {
        if !index.is_unique {
            return Err("SQL Server UNIQUE constraint metadata must be unique".into());
        }
        (
            format!("ALTER TABLE {table} ADD CONSTRAINT {name} UNIQUE {method} ({columns})"),
            format!("ALTER TABLE {table} DROP CONSTRAINT {name}"),
        )
    } else {
        (
            format!(
                "CREATE {}{method} INDEX {name} ON {table} ({columns})",
                if index.is_unique { "UNIQUE " } else { "" }
            ),
            format!("DROP INDEX {name} ON {table}"),
        )
    };
    Ok(MigrationStatement {
        sql,
        risk: MigrationRisk::Additive,
        rollback_sql: Some(rollback_sql),
        summary: format!("CREATE INDEX {} on {table}", index.name),
    })
}
