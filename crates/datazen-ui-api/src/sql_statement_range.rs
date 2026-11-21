pub const STATEMENT_RANGE_GAP: [&str; 2] = [
    "getStatementTextAtCursor (extension-points)",
    "unified semantic statement-range module",
];

pub type StatementAtCursorFn = fn(&str, usize) -> Option<String>;

pub fn get_statement_at_cursor<F>(sql: &str, cursor: usize, resolve: F) -> Option<String>
where
    F: Fn(&str, usize) -> Option<String>,
{
    resolve(sql, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_surface_is_pinned() {
        assert_eq!(STATEMENT_RANGE_GAP.len(), 2);
        assert!(STATEMENT_RANGE_GAP.contains(&"getStatementTextAtCursor (extension-points)"));
        assert_eq!(crate::STATEMENT_RANGE_GAP.len(), 2);
    }

    #[test]
    fn alias_delegates_to_the_injected_resolver() {
        let value = get_statement_at_cursor("select 1; select 2", 12, |sql, cursor| {
            assert_eq!(sql, "select 1; select 2");
            assert_eq!(cursor, 12);
            Some("select 2".to_owned())
        });
        assert_eq!(value.as_deref(), Some("select 2"));
        let none = get_statement_at_cursor("", 0, |_, _| None);
        assert_eq!(none, None);
    }
}
