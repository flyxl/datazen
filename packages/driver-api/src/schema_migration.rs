//! Dialect-neutral schema migration contracts exposed by the driver API.

use crate::{ColumnSchema, IndexInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_auto_increment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOperation {
    CreateTable {
        table: String,
        columns: Vec<MigrationColumn>,
        primary_keys: Vec<String>,
    },
    AddColumn {
        table: String,
        column: MigrationColumn,
    },
    DropColumn {
        table: String,
        column: MigrationColumn,
    },
    AlterColumnType {
        table: String,
        column: String,
        from: String,
        to: String,
    },
    SetNullable {
        table: String,
        column: String,
        nullable: bool,
    },
    SetDefault {
        table: String,
        column: String,
        from: Option<String>,
        to: Option<String>,
    },
    SetComment {
        table: String,
        column: String,
        from: Option<String>,
        to: Option<String>,
    },
    SetAutoIncrement {
        table: String,
        column: String,
        from: bool,
        to: bool,
    },
    AddPrimaryKey {
        table: String,
        columns: Vec<String>,
    },
    DropPrimaryKey {
        table: String,
        columns: Vec<String>,
    },
    CreateIndex {
        table: String,
        index: IndexInfo,
    },
    DropIndex {
        table: String,
        index: IndexInfo,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationRisk {
    Additive,
    Rewrite,
    Destructive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRequirement {
    Backfill {
        table: String,
        column: String,
        reason: String,
    },
    Unsupported {
        operation: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatement {
    pub sql: String,
    pub risk: MigrationRisk,
    pub rollback_sql: Option<String>,
    pub summary: String,
}

pub trait MigrationRenderer: Send + Sync {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String>;
}

pub trait MigrationCapabilities: Send + Sync {
    fn supports(&self, operation: &MigrationOperation) -> bool;
    fn requires_table_rebuild(&self, _operation: &MigrationOperation) -> bool {
        false
    }
    fn transactional_ddl(&self) -> bool {
        true
    }
}

pub fn migration_column(column: &ColumnSchema) -> MigrationColumn {
    MigrationColumn {
        name: column.name.clone(),
        data_type: column.data_type.clone(),
        nullable: column.nullable,
        default_value: column.default_value.clone(),
        comment: column.comment.clone(),
        is_auto_increment: column.is_auto_increment,
    }
}

/// Normalize a column type string for comparison purposes.
pub trait TypeNormalizer: Send + Sync {
    fn normalize_type(&self, data_type: &str) -> String;
}

/// Parse a type string into (base, args, suffix) components.
/// Example: `"VARCHAR(255) UNSIGNED"` → `("VARCHAR", Some("255"), "UNSIGNED")`
pub fn parse_type_parts(raw: &str) -> (String, Option<String>, String) {
    let trimmed = collapse_ws(raw);
    let (core, suffix) = peel_suffixes(&trimmed);
    match (core.find('('), core.rfind(')')) {
        (Some(open), Some(close)) if close > open => (
            core[..open].trim().to_string(),
            Some(core[open + 1..close].trim().to_string()),
            suffix,
        ),
        _ => (core, None, suffix),
    }
}

pub fn format_type(base: &str, args: Option<&str>, suffix: &str) -> String {
    let mut out = base.to_string();
    if let Some(a) = args {
        if !a.is_empty() {
            out.push('(');
            out.push_str(a);
            out.push(')');
        }
    }
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    out
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

#[cfg(test)]
mod type_parts_tests {
    use super::*;

    #[test]
    fn parse_varchar_with_unsigned_suffix() {
        let (base, args, suffix) = parse_type_parts("VARCHAR(255) UNSIGNED");
        assert_eq!(base, "VARCHAR");
        assert_eq!(args.as_deref(), Some("255"));
        assert_eq!(suffix, "UNSIGNED");
        assert_eq!(
            format_type(&base, args.as_deref(), &suffix),
            "VARCHAR(255) UNSIGNED"
        );
    }

    #[test]
    fn parse_multi_word_base() {
        let (base, args, suffix) = parse_type_parts("double precision");
        assert_eq!(base, "DOUBLE PRECISION");
        assert!(args.is_none());
        assert!(suffix.is_empty());
    }

    #[test]
    fn empty_input_yields_empty_base() {
        let (base, args, suffix) = parse_type_parts("  ");
        assert!(base.is_empty());
        assert!(args.is_none());
        assert!(suffix.is_empty());
    }
}
