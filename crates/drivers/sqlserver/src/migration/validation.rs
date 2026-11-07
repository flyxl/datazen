use super::*;
use std::collections::HashSet;

pub(super) fn quote_ident(value: &str) -> Result<String, String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(
            "SQL Server migration identifier is empty or contains control characters".into(),
        );
    }
    Ok(format!("[{}]", value.replace(']', "]]")))
}

pub(super) fn quote_path(value: &str) -> Result<Vec<String>, String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut parts = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        while chars.get(index).is_some_and(|ch| ch.is_whitespace()) {
            index += 1;
        }
        if index == chars.len() {
            return Err("SQL Server relation identity has an empty segment".into());
        }
        let mut segment = String::new();
        if chars[index] == '[' {
            index += 1;
            let mut closed = false;
            while index < chars.len() {
                match chars[index] {
                    ']' if chars.get(index + 1) == Some(&']') => {
                        segment.push(']');
                        index += 2;
                    }
                    ']' => {
                        closed = true;
                        index += 1;
                        break;
                    }
                    ch => {
                        segment.push(ch);
                        index += 1;
                    }
                }
            }
            if !closed {
                return Err(
                    "SQL Server relation identity has an unterminated bracketed segment".into(),
                );
            }
            if chars
                .get(index)
                .is_some_and(|ch| !ch.is_whitespace() && *ch != '.')
            {
                return Err(
                    "SQL Server relation identity has text after a bracketed segment".into(),
                );
            }
        } else {
            while index < chars.len() && chars[index] != '.' {
                if chars[index] == '[' || chars[index] == ']' {
                    return Err("SQL Server relation identity has an unmatched bracket".into());
                }
                segment.push(chars[index]);
                index += 1;
            }
            segment = segment.trim().to_owned();
        }
        if segment.is_empty() || segment.chars().any(char::is_control) {
            return Err("SQL Server relation identity contains an empty or invalid segment".into());
        }
        parts.push(quote_ident(&segment)?);
        while chars.get(index).is_some_and(|ch| ch.is_whitespace()) {
            index += 1;
        }
        if index < chars.len() {
            if chars[index] != '.' {
                return Err("SQL Server relation identity is malformed".into());
            }
            index += 1;
            if index == chars.len() {
                return Err("SQL Server relation identity has an empty segment".into());
            }
        }
    }
    if !(2..=3).contains(&parts.len()) {
        return Err("SQL Server migration requires an explicit schema-qualified relation".into());
    }
    Ok(parts)
}

pub(super) fn relation(value: &str) -> Result<String, String> {
    Ok(quote_path(value)?.join("."))
}

pub(super) fn relation_parts(value: &str) -> Result<Vec<String>, String> {
    // quote_path deliberately returns rendered components; decode only the
    // known bracket escaping so catalog scope can be addressed without
    // splitting a schema or table name containing a dot.
    let quoted = quote_path(value)?;
    quoted
        .iter()
        .map(|part| {
            let inner = part
                .strip_prefix('[')
                .and_then(|part| part.strip_suffix(']'))
                .ok_or("invalid quoted SQL Server relation component")?;
            Ok(inner.replace("]]", "]"))
        })
        .collect()
}

pub(super) fn relation_catalog_prefix(parts: &[String]) -> String {
    if parts.len() == 3 {
        format!("{}.", parts[0])
    } else {
        String::new()
    }
}

pub(super) fn sql_string(value: &str) -> String {
    format!("N'{}'", value.replace('\'', "''"))
}

pub(super) fn literal_expression(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
        || has_executable_separator(trimmed)
    {
        return Err(format!(
            "SQL Server {label} is empty or contains unsafe SQL"
        ));
    }
    if trimmed.to_ascii_uppercase().contains("NEXT VALUE FOR") {
        return Err(format!(
            "SQL Server {label} depends on a sequence reference that this migration plan cannot verify"
        ));
    }
    Ok(trimmed.to_owned())
}

/// Detect statement separators and SQL comments outside quoted strings and
/// identifiers. Semicolons inside a string literal remain valid data.
pub(super) fn has_executable_separator(value: &str) -> bool {
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut bracket_quoted = false;
    while index < chars.len() {
        let ch = chars[index];
        if single_quoted {
            if ch == '\'' && chars.get(index + 1) == Some(&'\'') {
                index += 2;
                continue;
            }
            if ch == '\'' {
                single_quoted = false;
            }
        } else if double_quoted {
            if ch == '"' && chars.get(index + 1) == Some(&'"') {
                index += 2;
                continue;
            }
            if ch == '"' {
                double_quoted = false;
            }
        } else if bracket_quoted {
            if ch == ']' && chars.get(index + 1) == Some(&']') {
                index += 2;
                continue;
            }
            if ch == ']' {
                bracket_quoted = false;
            }
        } else {
            match ch {
                '\'' => single_quoted = true,
                '"' => double_quoted = true,
                '[' => bracket_quoted = true,
                ';' => return true,
                '-' if chars.get(index + 1) == Some(&'-') => return true,
                '/' if chars.get(index + 1) == Some(&'*') => return true,
                '*' if chars.get(index + 1) == Some(&'/') => return true,
                _ => {}
            }
        }
        index += 1;
    }
    single_quoted || double_quoted || bracket_quoted
}

pub(super) fn validate_type(raw: &str) -> Result<String, String> {
    let (base, args, suffix) = parse_type_parts(raw);
    if !suffix.is_empty() {
        return Err(format!(
            "SQL Server type suffix '{suffix}' is outside the migration type contract"
        ));
    }
    let base = base.to_ascii_uppercase();
    let allowed = matches!(
        base.as_str(),
        "BIGINT"
            | "INT"
            | "INTEGER"
            | "SMALLINT"
            | "TINYINT"
            | "BIT"
            | "DECIMAL"
            | "NUMERIC"
            | "MONEY"
            | "SMALLMONEY"
            | "FLOAT"
            | "REAL"
            | "DATE"
            | "TIME"
            | "SMALLDATETIME"
            | "DATETIME"
            | "DATETIME2"
            | "DATETIMEOFFSET"
            | "CHAR"
            | "VARCHAR"
            | "NCHAR"
            | "NVARCHAR"
            | "BINARY"
            | "VARBINARY"
            | "UNIQUEIDENTIFIER"
    );
    if !allowed {
        return Err(format!(
            "SQL Server type '{raw}' is outside the representable migration subset"
        ));
    }
    let args = args.map(|args| {
        args.split(',')
            .map(str::trim)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    });
    let normalized_args = match (base.as_str(), args.as_deref()) {
        ("DECIMAL" | "NUMERIC", Some([precision, scale]))
            if precision
                .parse::<u8>()
                .is_ok_and(|value| (1..=38).contains(&value))
                && scale.parse::<u8>().is_ok_and(|value| value <= 38) =>
        {
            let precision = precision.parse::<u8>().map_err(|_| "invalid precision")?;
            let scale = scale.parse::<u8>().map_err(|_| "invalid scale")?;
            if scale > precision {
                return Err("SQL Server decimal scale exceeds precision".into());
            }
            Some(format!("{precision},{scale}"))
        }
        ("DECIMAL" | "NUMERIC", Some([precision]))
            if precision
                .parse::<u8>()
                .is_ok_and(|value| (1..=38).contains(&value)) =>
        {
            Some(precision.to_string())
        }
        ("FLOAT", Some([precision]))
            if precision
                .parse::<u8>()
                .is_ok_and(|value| (1..=53).contains(&value)) =>
        {
            Some(precision.to_string())
        }
        ("TIME" | "DATETIME2" | "DATETIMEOFFSET", Some([scale]))
            if scale.parse::<u8>().is_ok_and(|value| value <= 7) =>
        {
            Some(scale.to_string())
        }
        ("CHAR" | "VARCHAR" | "BINARY" | "VARBINARY", Some([length]))
            if length
                .parse::<u16>()
                .is_ok_and(|value| value > 0 && value <= 8000) =>
        {
            Some(length.to_ascii_uppercase())
        }
        ("NCHAR" | "NVARCHAR", Some([length]))
            if length
                .parse::<u16>()
                .is_ok_and(|value| value > 0 && value <= 4000) =>
        {
            Some(length.to_ascii_uppercase())
        }
        ("VARCHAR" | "VARBINARY" | "NVARCHAR", Some([length])) if *length == "MAX" => {
            Some(length.to_ascii_uppercase())
        }
        ("DECIMAL" | "NUMERIC" | "FLOAT" | "TIME" | "DATETIME2" | "DATETIMEOFFSET", None) => None,
        ("CHAR" | "VARCHAR" | "NCHAR" | "NVARCHAR" | "BINARY" | "VARBINARY", None) => {
            return Err(format!("SQL Server type '{base}' requires a length"));
        }
        (_, None) => None,
        _ => return Err(format!("invalid SQL Server type dimensions in '{raw}'")),
    };
    Ok(format_type(&base, normalized_args.as_deref(), ""))
}

pub(super) fn validate_column(column: &MigrationColumn) -> Result<String, String> {
    let name = quote_ident(&column.name)?;
    let data_type = validate_type(&column.data_type)?;
    if column.is_auto_increment && column.default_value.is_some() {
        return Err(format!(
            "SQL Server IDENTITY column '{}' cannot also have a DEFAULT constraint",
            column.name
        ));
    }
    let mut definition = format!(
        "{name} {data_type}{}{}",
        if column.is_auto_increment {
            " IDENTITY(1,1)"
        } else {
            ""
        },
        if column.nullable {
            " NULL"
        } else {
            " NOT NULL"
        }
    );
    if let Some(default) = &column.default_value {
        definition.push_str(" DEFAULT ");
        definition.push_str(&literal_expression(default, "DEFAULT expression")?);
    }
    Ok(definition)
}

pub(super) fn validate_unique_columns(columns: &[String], label: &str) -> Result<(), String> {
    if columns.is_empty() {
        return Err(format!(
            "SQL Server {label} must include at least one column"
        ));
    }
    let mut seen = HashSet::new();
    for column in columns {
        quote_ident(column)?;
        if !seen.insert(column) {
            return Err(format!("SQL Server {label} repeats column '{column}'"));
        }
    }
    Ok(())
}

pub(super) fn table_options_supported(options: &TableOptions) -> bool {
    options.comment.is_none()
        && options.engine.is_none()
        && options.charset.is_none()
        && options.collation.is_none()
        && options.migration_blockers.is_empty()
}
