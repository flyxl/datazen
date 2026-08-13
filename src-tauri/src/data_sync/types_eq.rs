//! Family-local type equivalence for the Data Sync structure gate.
//!
//! Host must not invent cross-family conversions (that is Transfer).

pub fn types_equivalent(family: &str, left: &str, right: &str) -> bool {
    canonical_type(family, left) == canonical_type(family, right)
}

pub fn canonical_type(family: &str, raw: &str) -> String {
    let parsed = parse_type(raw);
    if parsed.base.is_empty() {
        return String::new();
    }

    let mut base = alias_base(family, &parsed.base);
    let mut args = parsed.args;
    let suffix = parsed.suffix;

    if family == "mysql" && is_mysql_integer(&base) {
        if args
            .as_deref()
            .is_some_and(|a| a.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()))
        {
            args = None;
        }
    }
    if base == "BOOL" || base == "BOOLEAN" {
        base = "BOOLEAN".into();
        args = None;
    }
    if family == "postgresql" {
        match base.as_str() {
            "CHARACTER VARYING" => base = "VARCHAR".into(),
            "CHARACTER" => base = "CHAR".into(),
            _ => {}
        }
    }

    let mut out = base;
    if let Some(a) = args {
        if !a.is_empty() {
            out.push('(');
            out.push_str(&a);
            out.push(')');
        }
    }
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(&suffix);
    }
    out
}

struct ParsedType {
    base: String,
    args: Option<String>,
    suffix: String,
}

fn parse_type(raw: &str) -> ParsedType {
    let trimmed = collapse_ws(raw);
    let (core, suffix) = peel_suffixes(&trimmed);
    match (core.find('('), core.rfind(')')) {
        (Some(open), Some(close)) if close > open => ParsedType {
            base: core[..open].trim().to_string(),
            args: Some(core[open + 1..close].trim().to_string()),
            suffix,
        },
        _ => ParsedType {
            base: core,
            args: None,
            suffix,
        },
    }
}

fn peel_suffixes(raw: &str) -> (String, String) {
    let mut parts: Vec<&str> = raw.split_whitespace().collect();
    let mut suffix = Vec::new();
    while let Some(last) = parts.last().copied() {
        if matches!(last, "UNSIGNED" | "ZEROFILL" | "BINARY") {
            suffix.push(parts.pop().expect("last token"));
        } else {
            break;
        }
    }
    suffix.reverse();
    (parts.join(" "), suffix.join(" "))
}

fn collapse_ws(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn alias_base(family: &str, base: &str) -> String {
    match family {
        "mysql" => match base {
            "INTEGER" => "INT".into(),
            "BOOL" => "BOOLEAN".into(),
            "DEC" => "DECIMAL".into(),
            "NUMERIC" => "DECIMAL".into(),
            other => other.to_string(),
        },
        "postgresql" => match base {
            "INT" | "INT4" => "INTEGER".into(),
            "INT8" => "BIGINT".into(),
            "INT2" => "SMALLINT".into(),
            "FLOAT8" | "DOUBLE PRECISION" => "DOUBLE PRECISION".into(),
            "FLOAT4" => "REAL".into(),
            "BOOL" => "BOOLEAN".into(),
            "CHARACTER VARYING" => "VARCHAR".into(),
            "CHARACTER" => "CHAR".into(),
            "TIMESTAMPTZ" => "TIMESTAMP WITH TIME ZONE".into(),
            "TIMESTAMP WITHOUT TIME ZONE" => "TIMESTAMP".into(),
            other => other.to_string(),
        },
        _ => base.to_string(),
    }
}

fn is_mysql_integer(base: &str) -> bool {
    matches!(
        base,
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_int_aliases_and_display_width() {
        assert!(types_equivalent("mysql", "INT", "INTEGER"));
        assert!(types_equivalent("mysql", "int(11)", "INT"));
        assert!(types_equivalent("mysql", "BIGINT(20)", "bigint"));
        assert!(!types_equivalent("mysql", "INT", "BIGINT"));
        assert!(!types_equivalent("mysql", "INT", "INT UNSIGNED"));
        assert!(!types_equivalent("mysql", "VARCHAR(20)", "TEXT"));
        assert!(!types_equivalent("mysql", "VARCHAR(20)", "VARCHAR(21)"));
        assert!(types_equivalent("mysql", "BOOL", "BOOLEAN"));
        assert!(!types_equivalent("mysql", "TINYINT(1)", "BOOLEAN"));
        assert!(types_equivalent("mysql", "DECIMAL(10,2)", "NUMERIC(10,2)"));
        assert!(!types_equivalent("mysql", "DECIMAL(10,2)", "DECIMAL(10,3)"));
        assert_eq!(canonical_type("mysql", "INT(11) UNSIGNED"), "INT UNSIGNED");
        assert!(types_equivalent(
            "mysql",
            "INT(11) UNSIGNED",
            "INTEGER UNSIGNED"
        ));
    }

    #[test]
    fn postgres_int_and_varchar_aliases() {
        assert!(types_equivalent("postgresql", "int", "INTEGER"));
        assert!(types_equivalent("postgresql", "int4", "integer"));
        assert!(types_equivalent("postgresql", "int8", "BIGINT"));
        assert!(types_equivalent("postgresql", "BOOL", "boolean"));
        assert!(types_equivalent(
            "postgresql",
            "character varying(20)",
            "VARCHAR(20)"
        ));
        assert!(!types_equivalent("postgresql", "VARCHAR(20)", "TEXT"));
        assert!(!types_equivalent("postgresql", "INTEGER", "BIGINT"));
        assert!(types_equivalent(
            "postgresql",
            "timestamptz",
            "TIMESTAMP WITH TIME ZONE"
        ));
        assert!(types_equivalent("postgresql", "float8", "double precision"));
        assert!(types_equivalent("postgresql", "float4", "real"));
        assert!(types_equivalent(
            "postgresql",
            "timestamp without time zone",
            "TIMESTAMP"
        ));
        assert!(types_equivalent("postgresql", "int2", "smallint"));
        assert!(types_equivalent("postgresql", "character(2)", "CHAR(2)"));
    }

    #[test]
    fn empty_and_unknown_family_are_literal() {
        assert!(types_equivalent("sqlite", "TEXT", "text"));
        assert_eq!(canonical_type("mysql", "  "), "");
        assert!(!types_equivalent("", "INT", "INTEGER"));
    }
}
