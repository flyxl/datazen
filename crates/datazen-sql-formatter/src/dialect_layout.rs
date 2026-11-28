//! Orthogonal dialect data from sql-formatter 15.8.2 (MIT).
//! Reference: languages/{postgresql,transactsql,mysql,mariadb}/*.formatter.js.
//! Lexing/layout and production feature gates remain separate.

use crate::Dialect;

pub(crate) fn is_property_access(dialect: Dialect, spelling: &str) -> bool {
    spelling == "." || (dialect == Dialect::TransactSql && spelling == "..")
}

pub(crate) fn is_always_dense(dialect: Dialect, spelling: &str) -> bool {
    match dialect {
        Dialect::PostgreSql => matches!(spelling, "::" | ":"),
        Dialect::TransactSql => spelling == "::",
        _ => false,
    }
}

/// The caller has already validated a real SELECT token and normalized its
/// whitespace/case. DistinctRow is an explicit MySQL/MariaDB SELECT head,
/// never a heuristic keyword rewrite of an identifier.
pub(crate) fn is_select_head(dialect: Dialect, canonical: &str) -> bool {
    matches!(canonical, "SELECT" | "SELECT ALL" | "SELECT DISTINCT")
        || (matches!(dialect, Dialect::MySql | Dialect::MariaDb)
            && canonical == "SELECT DISTINCTROW")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_cast_slice_and_property_spacing_are_dialect_scoped() {
        assert!(is_always_dense(Dialect::PostgreSql, "::"));
        assert!(is_always_dense(Dialect::PostgreSql, ":"));
        assert!(is_always_dense(Dialect::TransactSql, "::"));
        assert!(!is_always_dense(Dialect::TransactSql, ":"));
        assert!(!is_always_dense(Dialect::PostgreSql, "->>"));
        assert!(!is_always_dense(Dialect::Sqlite, "::"));
        assert!(is_property_access(Dialect::TransactSql, ".."));
        assert!(is_property_access(Dialect::Sqlite, "."));
        assert!(!is_property_access(Dialect::PostgreSql, ".."));
    }

    #[test]
    fn original_select_distinctrow_is_not_a_generic_select_modifier() {
        assert!(is_select_head(Dialect::MySql, "SELECT DISTINCTROW"));
        assert!(is_select_head(Dialect::MariaDb, "SELECT DISTINCTROW"));
        assert!(!is_select_head(Dialect::Sqlite, "SELECT DISTINCTROW"));
        assert!(is_select_head(Dialect::Sql, "SELECT DISTINCT"));
        assert!(!is_select_head(Dialect::Sql, "WITH"));
    }
}
