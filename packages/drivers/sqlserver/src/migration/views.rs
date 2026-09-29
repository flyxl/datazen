use super::validation::*;
use super::*;

pub(super) fn validate_view(view: &MigrationView) -> Result<(String, String), String> {
    let schema = view
        .schema
        .as_deref()
        .filter(|schema| !schema.is_empty())
        .ok_or("SQL Server view migration requires its exact schema")?;
    let ident = format!("{}.{}", quote_ident(schema)?, quote_ident(&view.name)?);
    let definition = view.definition.trim();
    if definition.is_empty() || has_executable_separator(definition) {
        return Err("SQL Server view definition must be one complete, safe batch".into());
    }
    let upper = definition.to_ascii_uppercase();
    let is_full_ddl = upper.starts_with("CREATE VIEW") || upper.starts_with("ALTER VIEW");
    if !is_full_ddl {
        if upper.starts_with("CREATE ") || upper.starts_with("ALTER ") {
            return Err(
                "SQL Server view DDL must use a supported CREATE VIEW or ALTER VIEW header".into(),
            );
        }
        validate_view_definition(definition)?;
        let sql = format!("CREATE VIEW {ident} AS {definition}");
        return Ok((ident, sql));
    }

    let (declared_schema, declared_name) = view_header_identity(definition)?;
    if declared_schema != schema || declared_name != view.name {
        return Err(format!(
            "SQL Server view definition identity `{}.{}` does not match requested `{}.{}`",
            declared_schema, declared_name, schema, view.name
        ));
    }
    let as_offset = find_header_as(definition)?;
    validate_view_definition(&definition[as_offset..])?;
    Ok((ident, definition.to_owned()))
}

pub(super) fn view_header_identity(definition: &str) -> Result<(String, String), String> {
    let bytes = definition.as_bytes();
    let mut index = 0;
    let first = read_word(bytes, &mut index)?;
    if !first.eq_ignore_ascii_case("CREATE") && !first.eq_ignore_ascii_case("ALTER") {
        return Err("SQL Server view definition must begin with CREATE VIEW or ALTER VIEW".into());
    }
    let second = read_word(bytes, &mut index)?;
    if !second.eq_ignore_ascii_case("VIEW") {
        return Err("SQL Server view definition must begin with CREATE VIEW or ALTER VIEW".into());
    }
    let parts = read_identifier_path(bytes, &mut index)?;
    if parts.len() != 2 {
        return Err(
            "SQL Server view definition must declare an exact schema-qualified identity".into(),
        );
    }
    Ok((parts[0].clone(), parts[1].clone()))
}

pub(super) fn read_word(bytes: &[u8], index: &mut usize) -> Result<String, String> {
    skip_space(bytes, index);
    let start = *index;
    while bytes
        .get(*index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'@' | b'#'))
    {
        *index += 1;
    }
    if *index == start {
        return Err("SQL Server object definition header is malformed".into());
    }
    Ok(String::from_utf8_lossy(&bytes[start..*index]).into_owned())
}

pub(super) fn skip_space(bytes: &[u8], index: &mut usize) {
    while bytes.get(*index).is_some_and(u8::is_ascii_whitespace) {
        *index += 1;
    }
}

pub(super) fn read_identifier_path(bytes: &[u8], index: &mut usize) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    loop {
        skip_space(bytes, index);
        let mut part = String::new();
        if bytes.get(*index) == Some(&b'[') {
            *index += 1;
            let mut start = *index;
            let mut closed = false;
            while let Some(byte) = bytes.get(*index).copied() {
                if byte == b']' && bytes.get(*index + 1) == Some(&b']') {
                    part.push_str(std::str::from_utf8(&bytes[start..*index]).map_err(|_| {
                        "SQL Server object definition identifier is not valid UTF-8"
                    })?);
                    part.push(']');
                    *index += 2;
                    start = *index;
                } else if byte == b']' {
                    part.push_str(std::str::from_utf8(&bytes[start..*index]).map_err(|_| {
                        "SQL Server object definition identifier is not valid UTF-8"
                    })?);
                    closed = true;
                    *index += 1;
                    break;
                } else {
                    *index += 1;
                }
            }
            if !closed {
                return Err("SQL Server object definition has an unterminated identifier".into());
            }
        } else {
            let start = *index;
            while bytes.get(*index).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && *byte != b'.' && *byte != b'[' && *byte != b']'
            }) {
                *index += 1;
            }
            if *index == start {
                return Err("SQL Server object definition identity is malformed".into());
            }
            part = String::from_utf8(bytes[start..*index].to_vec())
                .map_err(|_| "SQL Server object definition identity is not valid UTF-8")?;
        }
        if part.is_empty() {
            return Err("SQL Server object definition identity has an empty segment".into());
        }
        parts.push(part);
        skip_space(bytes, index);
        if bytes.get(*index) != Some(&b'.') {
            break;
        }
        *index += 1;
    }
    Ok(parts)
}

pub(super) fn find_header_as(definition: &str) -> Result<usize, String> {
    let bytes = definition.as_bytes();
    let mut index = 0;
    let _ = read_word(bytes, &mut index)?;
    let _ = read_word(bytes, &mut index)?;
    let _ = read_identifier_path(bytes, &mut index)?;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_bracket = false;
    let mut depth = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_single {
            if byte == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
                continue;
            }
            if byte == b'\'' {
                in_single = false;
            }
        } else if in_double {
            if byte == b'"' && bytes.get(index + 1) == Some(&b'"') {
                index += 2;
                continue;
            }
            if byte == b'"' {
                in_double = false;
            }
        } else if in_bracket {
            if byte == b']' && bytes.get(index + 1) == Some(&b']') {
                index += 2;
                continue;
            }
            if byte == b']' {
                in_bracket = false;
            }
        } else {
            match byte {
                b'\'' => in_single = true,
                b'"' => in_double = true,
                b'[' => in_bracket = true,
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                b'A' | b'a' if depth == 0 => {
                    let after_word = index + 2;
                    let after = bytes
                        .get(after_word)
                        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'(');
                    if after
                        && bytes
                            .get(index..after_word)
                            .is_some_and(|word| word.eq_ignore_ascii_case(b"as"))
                    {
                        return Ok(after_word);
                    }
                }
                b';' => {
                    return Err("SQL Server view definition contains multiple statements".into())
                }
                _ => {}
            }
        }
        index += 1;
    }
    Err("SQL Server view definition is missing its AS clause".into())
}

pub(super) fn to_alter_view(definition: &str) -> Result<String, String> {
    let trimmed = definition.trim_start();
    if trimmed.len() >= 6 && trimmed[..6].eq_ignore_ascii_case("CREATE") {
        Ok(format!("ALTER{}", &trimmed[6..]))
    } else if trimmed.len() >= 5 && trimmed[..5].eq_ignore_ascii_case("ALTER") {
        Ok(trimmed.to_owned())
    } else {
        Err("SQL Server replacement view requires a CREATE/ALTER VIEW definition".into())
    }
}
