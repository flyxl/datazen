//! Exact SQLite statement framing using the linked engine's pure completion parser.
use std::ops::Range;

use datazen_driver_api::sql_split::is_comment_only_or_empty;
use datazen_driver_api::DriverError;

/// Return byte spans, excluding separators between statements. SQLite itself
/// recognizes delimiters, so quoted text, identifiers, comments and a
/// CREATE TRIGGER body keep their literal semicolons.
pub fn sqlite_statement_ranges(sql: &str) -> Result<Vec<Range<usize>>, DriverError> {
    if sql.as_bytes().contains(&0) {
        return Err(DriverError::QueryFailed(
            "SQL text contains a NUL byte.".into(),
        ));
    }
    let mut bytes = sql.as_bytes().to_vec();
    // A tail without a delimiter is normal in the editor. The newline also
    // terminates a trailing line comment before the synthetic separator.
    bytes.extend_from_slice(b"\n;\0");
    let mut start = 0;
    let mut result = Vec::new();
    // sqlite3_complete rescans the current fragment at every candidate. Bound
    // total work for pathological quotes/comments with many internal semicolons.
    let mut budget = sql.len().saturating_mul(16).saturating_add(1024 * 1024);
    for (end, byte) in sql.bytes().enumerate() {
        if byte != b';' {
            continue;
        }
        budget = budget.checked_sub(end + 1 - start).ok_or_else(|| {
            DriverError::QueryFailed("SQLite statement framing exceeded its scan budget.".into())
        })?;
        let terminator = bytes[end + 1];
        bytes[end + 1] = 0;
        let is_complete = complete(&bytes[start..]);
        bytes[end + 1] = terminator;
        if is_complete {
            if !is_comment_only_or_empty(&sql[start..end]) {
                result.push(start..end);
            }
            start = end + 1;
        }
    }
    if !is_comment_only_or_empty(&sql[start..]) {
        if !complete(&bytes[start..]) {
            return Err(DriverError::QueryFailed(
                "The SQLite statement is not complete.".into(),
            ));
        }
        result.push(start..sql.len());
    }
    Ok(result)
}

fn complete(nul_terminated: &[u8]) -> bool {
    // SAFETY: the live byte slice always contains a NUL terminator, either
    // just after the candidate separator or at the end of the synthetic tail.
    // SQLite does not retain the pointer and no database handle is involved.
    unsafe { libsqlite3_sys::sqlite3_complete(nul_terminated.as_ptr().cast()) == 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statements(sql: &str) -> Vec<&str> {
        sqlite_statement_ranges(sql)
            .unwrap()
            .into_iter()
            .map(|range| sql[range].trim())
            .collect()
    }

    #[test]
    fn retains_literals_and_identifier_delimiters_at_utf8_byte_offsets() {
        assert_eq!(
            statements("SELECT '汉;字' AS [别;名]; /* ; */ SELECT `two;x`"),
            ["SELECT '汉;字' AS [别;名]", "/* ; */ SELECT `two;x`"]
        );
        assert_eq!(
            statements("SELECT 'it''s;x'; -- done;"),
            ["SELECT 'it''s;x'"]
        );
        assert_eq!(
            statements("SELECT 1 -- no delimiter"),
            ["SELECT 1 -- no delimiter"]
        );
    }

    #[test]
    fn groups_sqlite_trigger_body_and_skips_empty_comments() {
        let trigger = "CREATE TRIGGER t AFTER INSERT ON r BEGIN INSERT INTO s VALUES(1); UPDATE s SET x=2; END";
        assert_eq!(
            statements(&format!(" ; -- ;\n{trigger}; SELECT 7;")),
            [format!("-- ;\n{trigger}"), "SELECT 7".to_owned()]
        );
        assert!(statements("-- ;\n; /* ; */ ;").is_empty());
    }

    #[test]
    fn refuses_unterminated_text_and_nul_without_framing_a_prefix() {
        assert!(sqlite_statement_ranges("SELECT 1; SELECT 'not;closed").is_err());
        assert!(sqlite_statement_ranges("SELECT 1;\0 SELECT 2").is_err());
    }
}
