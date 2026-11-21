mod scan;

pub use scan::{looks_like_sql, starts_sql_statement};

pub fn extract_sql_from_response(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(inner) = scan::find_fence(trimmed, true, true) {
        return inner.trim().to_owned();
    }
    if let Some(inner) = scan::find_fence(trimmed, false, true) {
        let inner = inner.trim();
        if scan::looks_like_sql(inner) {
            return inner.to_owned();
        }
    }
    if scan::looks_like_sql(trimmed) {
        return trimmed.to_owned();
    }
    let extracted = scan::extract_sql_lines(trimmed);
    let extracted = extracted.trim();
    if !extracted.is_empty() {
        return extracted.to_owned();
    }
    trimmed.to_owned()
}

pub fn extract_sql_streaming(accumulated: &str) -> String {
    if accumulated.is_empty() {
        return String::new();
    }
    if let Some(inner) = scan::find_fence(accumulated, true, true) {
        return inner;
    }
    if let Some(inner) = scan::find_fence(accumulated, true, false) {
        return inner;
    }
    if let Some(inner) = scan::find_fence(accumulated, false, true) {
        if scan::looks_like_sql(&inner) {
            return inner;
        }
    }
    if let Some(inner) = scan::find_fence(accumulated, false, false) {
        if scan::looks_like_sql(&inner) {
            return inner;
        }
    }
    let extracted = scan::extract_sql_lines(accumulated);
    if !extracted.is_empty() {
        return extracted;
    }
    accumulated.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_response_yields_an_empty_string() {
        assert_eq!(extract_sql_from_response(""), "");
        assert_eq!(extract_sql_from_response("   \n\t"), "");
    }

    #[test]
    fn a_sql_language_fence_wins_over_everything_else() {
        let raw = "Sure! Here it is:\n```sql\nSELECT a FROM t;\n```\nHope that helps.";
        assert_eq!(extract_sql_from_response(raw), "SELECT a FROM t;");
        assert_eq!(extract_sql_streaming(raw), "SELECT a FROM t;\n");
    }

    #[test]
    fn every_sql_family_tag_is_recognized() {
        for tag in ["sql", "SQL", "mysql", "postgresql", "postgres", "sqlite"] {
            let raw = format!("```{tag}\nSELECT 1\n```");
            assert_eq!(extract_sql_from_response(&raw), "SELECT 1", "{tag}");
        }
    }

    #[test]
    fn a_bare_fence_is_used_when_its_body_looks_like_sql() {
        let raw = "text\n```\nSELECT 1\n```\ntext";
        assert_eq!(extract_sql_from_response(raw), "SELECT 1");
    }

    #[test]
    fn a_bare_fence_without_sql_is_ignored() {
        let raw = "text\n```\nnot sql at all\n```\nSELECT 2";
        assert_eq!(extract_sql_from_response(raw), "SELECT 2");
    }

    #[test]
    fn a_whole_response_that_looks_like_sql_is_returned_trimmed() {
        assert_eq!(extract_sql_from_response("  SELECT 1  "), "SELECT 1");
        assert_eq!(extract_sql_from_response("update t set a = 1"), "update t set a = 1");
    }

    #[test]
    fn mixed_content_keeps_only_statement_openers_when_indent_check_sees_trimmed_lines() {
        let raw = "Here is the query:\nSELECT a,\n  b\nFROM t;\nThat is all.";
        assert_eq!(extract_sql_from_response(raw), "SELECT a,");
    }

    #[test]
    fn blank_lines_inside_an_sql_block_are_kept_but_outer_blanks_are_trimmed() {
        let raw = "intro\nSELECT 1\n\nFROM t\noutro";
        assert_eq!(extract_sql_from_response(raw), "SELECT 1\n\nFROM t");
    }

    #[test]
    fn a_response_without_sql_falls_back_to_the_trimmed_input() {
        assert_eq!(extract_sql_from_response("  just prose  "), "just prose");
        assert_eq!(extract_sql_from_response("nothing: useful"), "nothing: useful");
    }

    #[test]
    fn streaming_keeps_unclosed_sql_fence_content_without_trimming() {
        assert_eq!(extract_sql_streaming("```sql\nSELECT 1\n  "), "SELECT 1\n  ");
        assert_eq!(extract_sql_streaming("```postgres\nSELECT 1"), "SELECT 1");
    }

    #[test]
    fn streaming_keeps_unclosed_bare_fence_content_when_it_looks_like_sql() {
        assert_eq!(extract_sql_streaming("```\nSELECT 1\n"), "SELECT 1\n");
        assert_eq!(extract_sql_streaming("```\nnot sql"), "```\nnot sql");
    }

    #[test]
    fn streaming_lines_are_not_trimmed_and_fall_back_is_the_raw_accumulated_text() {
        assert_eq!(extract_sql_streaming("  SELECT 1  "), "  SELECT 1  ");
        assert_eq!(extract_sql_streaming("  prose  "), "  prose  ");
        assert_eq!(extract_sql_streaming(""), "");
    }

    #[test]
    fn streaming_prefers_a_closed_fence_over_an_unclosed_one() {
        let raw = "```sql\nSELECT 1\n```\n```sql\nSELECT 2";
        assert_eq!(extract_sql_streaming(raw), "SELECT 1\n");
    }
}
