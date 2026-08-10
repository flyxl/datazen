//! Intermediate Representation for cross-database type mapping.
//!
//! Each database adapter converts native types to/from this IR,
//! reducing N×N direct mappings to N adapters (O(N) instead of O(N²)).

/// Database-agnostic column type.
#[derive(Debug, Clone, PartialEq)]
pub enum IRType {
    Bool,

    Int8,
    Int16,
    Int32,
    Int64,

    Float32,
    Float64,

    /// Exact numeric with optional precision/scale. `precision == 0` means unbounded.
    Decimal { precision: u8, scale: u8 },

    Char { length: u32 },
    /// `None` length means unbounded (equivalent to TEXT in most databases).
    Varchar { length: Option<u32> },
    Text,

    Binary { length: Option<u32> },
    Blob,

    Date,
    Time { with_timezone: bool },
    Timestamp { with_timezone: bool },

    Json,
    Uuid,

    Bit { length: u32 },

    /// Fallback for types that have no standard IR mapping.
    /// Target adapters should degrade this to TEXT.
    Other(String),
}

/// Standardised default-value expression.
#[derive(Debug, Clone, PartialEq)]
pub enum IRDefault {
    CurrentTimestamp,
    /// A literal value stripped of any database-specific cast syntax.
    Literal(String),
    /// An expression that could not be normalised (e.g. sequence calls).
    RawExpression(String),
}

/// One column in the intermediate schema.
#[derive(Debug, Clone)]
pub struct IRColumn {
    pub name: String,
    pub ir_type: IRType,
    pub nullable: bool,
    pub default_expr: Option<IRDefault>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub comment: Option<String>,
}

/// A full table definition in intermediate form.
#[derive(Debug, Clone)]
pub struct IRTable {
    pub name: String,
    pub columns: Vec<IRColumn>,
    pub primary_keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ir_type_equality() {
        assert_eq!(
            IRType::Varchar { length: Some(255) },
            IRType::Varchar { length: Some(255) },
        );
        assert_ne!(
            IRType::Varchar { length: Some(255) },
            IRType::Varchar { length: None },
        );
    }

    #[test]
    fn ir_default_equality() {
        assert_eq!(IRDefault::CurrentTimestamp, IRDefault::CurrentTimestamp);
        assert_ne!(
            IRDefault::Literal("42".into()),
            IRDefault::RawExpression("42".into()),
        );
    }
}
