//! Port of `packages/frontend/src/lib/aiMessageBlocks.ts` (T228) — parse AI
//! chat message content into alternating text and fenced-code segments
//! (standard markdown fences: `` ```lang\n…\n``` ``), plus the SQL probe that
//! decides whether a code block offers "Insert to Editor".
//!
//! React drives the split with one regex —
//! `` /```([\w-]*)\s*\n([\s\S]*?)```/g `` — and the port keeps its exact
//! semantics with a hand-written scanner:
//!
//! - the language tag is the `[\w-]*` word run after the fence opener;
//! - the tag's trailing whitespace run must contain a newline (the `\s*\n`
//!   leg — greedy `\s*` backtracking to keep one newline leaves the match
//!   ending at the run's **last** newline, so the body starts right after
//!   it; a fence whose tag run hits neither newline nor another fence is
//!   not a fence, and the scan continues at the next opener);
//! - the body is the lazy `[\s\S]*?` run to the **nearest** closing fence;
//!   an opener with no closer produces no segment (the regex engine keeps
//!   scanning for later complete matches);
//! - the body's trailing whitespace dies (`trimEnd()`), the language tag
//!   is lowercased.
//!
//! The React suite (`__tests__/aiMessageBlocks.test.ts`) ports below.

/// React `CodeBlockSegment` — one fenced code block: the language tag
/// (lowercased) and the trimmed body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlockSegment {
    pub language: String,
    pub code: String,
}

/// React `TextSegment` — the prose between fences, verbatim (order and
/// trailing whitespace preserved).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSegment {
    pub content: String,
}

/// React `MessageSegment = TextSegment | CodeBlockSegment` — the alternating
/// pair the renderer walks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageSegment {
    Text(TextSegment),
    Code(CodeBlockSegment),
}

#[inline]
fn is_language_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

#[inline]
fn is_ws_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Find the next ``` fence opener at or after `from`.
fn find_fence_opener(bytes: &[u8], from: usize) -> Option<usize> {
    (from..bytes.len().saturating_sub(2)).find(|&at| &bytes[at..at + 3] == b"```")
}

/// Split message content into text and code segments, preserving order.
pub fn parse_message_segments(text: &str) -> Vec<MessageSegment> {
    if text.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut segments: Vec<MessageSegment> = Vec::new();
    let mut last = 0usize;
    let mut cursor = 0usize;
    while let Some(at) = find_fence_opener(bytes, cursor) {
        // The language tag run.
        let mut tag_end = at + 3;
        while tag_end < bytes.len() && is_language_byte(bytes[tag_end]) {
            tag_end += 1;
        }
        // The `\s*\n` leg: the whitespace run after the tag must contain a
        // newline, and the body starts right after the run's last newline
        // (the greedy-backtracking reading of the regex).
        let mut ws_end = tag_end;
        let mut last_newline: Option<usize> = None;
        while ws_end < bytes.len() && is_ws_byte(bytes[ws_end]) {
            if bytes[ws_end] == b'\n' {
                last_newline = Some(ws_end);
            }
            ws_end += 1;
        }
        let Some(newline) = last_newline else {
            // Not a fence: the regex engine keeps scanning past this opener.
            cursor = at + 3;
            continue;
        };
        // The lazy body: to the nearest closing fence.
        let body_start = newline + 1;
        let Some(close) = find_fence_opener(bytes, body_start) else {
            // Unclosed: no segment from this opener; keep scanning later ones.
            cursor = at + 3;
            continue;
        };
        if at > last {
            let before = &text[last..at];
            if !before.is_empty() {
                segments.push(MessageSegment::Text(TextSegment {
                    content: before.to_owned(),
                }));
            }
        }
        segments.push(MessageSegment::Code(CodeBlockSegment {
            language: text[at + 3..tag_end].to_ascii_lowercase(),
            code: text[body_start..close].trim_end().to_owned(),
        }));
        last = close + 3;
        cursor = last;
    }
    let tail = &text[last.min(text.len())..];
    if !tail.is_empty() {
        segments.push(MessageSegment::Text(TextSegment {
            content: tail.to_owned(),
        }));
    }
    segments
}

/// React's SQL language set — the tags whose blocks always offer Insert to
/// Editor.
const SQL_LANGS: [&str; 6] = ["sql", "mysql", "postgresql", "postgres", "sqlite", "mariadb"];

/// React's leading-statement keyword list — the `looksLikeSql` regex's
/// alternation, verbatim (21 keywords).
const SQL_KEYWORDS: [&str; 21] = [
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "EXPLAIN", "SHOW",
    "DESCRIBE", "PRAGMA", "TRUNCATE", "GRANT", "REVOKE", "BEGIN", "COMMIT", "ROLLBACK", "SET",
    "USE", "VACUUM",
];

/// React `isSqlCodeBlock` — whether a fenced code block should offer
/// "Insert to Editor": a SQL-family language tag, or a body whose first
/// line opens with a leading SQL statement keyword (case-insensitive, word
/// boundary).
pub fn is_sql_code_block(language: &str, code: &str) -> bool {
    let language = language.to_ascii_lowercase();
    if SQL_LANGS.contains(&language.as_str()) {
        return true;
    }
    looks_like_sql(code)
}

/// React `looksLikeSql` — the first line (trimmed) opens with one of the
/// leading statement keywords followed by a word boundary (a keyword
/// followed by another word character, like `SELECT1`, is not one).
fn looks_like_sql(text: &str) -> bool {
    let first_line = text.split('\n').next().unwrap_or_default().trim();
    let upper = first_line.to_ascii_uppercase();
    SQL_KEYWORDS.iter().any(|keyword| {
        if let Some(rest) = upper.strip_prefix(keyword) {
            // \b: the keyword ends at the line's end or at a non-word byte.
            match rest.as_bytes().first() {
                None => true,
                Some(next) => !(next.is_ascii_alphanumeric() || *next == b'_'),
            }
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(content: &str) -> MessageSegment {
        MessageSegment::Text(TextSegment {
            content: content.to_owned(),
        })
    }

    fn code(language: &str, body: &str) -> MessageSegment {
        MessageSegment::Code(CodeBlockSegment {
            language: language.to_owned(),
            code: body.to_owned(),
        })
    }

    #[test]
    fn returns_empty_for_empty_string() {
        assert_eq!(parse_message_segments(""), Vec::new());
    }

    #[test]
    fn returns_single_text_segment_when_no_fences() {
        assert_eq!(
            parse_message_segments("Hello world"),
            vec![text("Hello world")]
        );
    }

    #[test]
    fn parses_one_sql_fence_with_surrounding_text() {
        let input = "Try:\n```sql\nSELECT 1\n```\nDone.";
        assert_eq!(
            parse_message_segments(input),
            vec![
                text("Try:\n"),
                code("sql", "SELECT 1"),
                text("\nDone."),
            ]
        );
    }

    #[test]
    fn parses_multiple_fences() {
        let input = "A\n```sql\nSELECT 1\n```\nB\n```json\n{\"a\":1}\n```\nC";
        let segments = parse_message_segments(input);
        assert_eq!(segments.len(), 5);
        assert_eq!(segments[1], code("sql", "SELECT 1"));
        assert_eq!(segments[3], code("json", "{\"a\":1}"));
    }

    #[test]
    fn handles_bare_fence_without_language_tag() {
        let input = "```\nSELECT 2\n```";
        assert_eq!(
            parse_message_segments(input),
            vec![code("", "SELECT 2")]
        );
    }

    #[test]
    fn preserves_trailing_whitespace_in_text_but_trims_code_trailing_newline() {
        let input = "text\n```sql\nSELECT 1;\n\n```\n";
        let segments = parse_message_segments(input);
        assert_eq!(segments[0], text("text\n"));
        assert_eq!(segments[1], code("sql", "SELECT 1;"));
    }

    /// Beyond the React suite — the regex's own edge rulings the suite does
    /// not reach: the lazy body takes the **nearest** closing fence, so an
    /// unterminated later block is swallowed as the earlier block's body
    /// (the engine then cannot match again: the trailing `text…```\nend`
    /// stays prose), and a truly unclosed opener is plain text.
    #[test]
    fn an_unclosed_later_fence_becomes_the_earlier_body_and_the_tail_stays_prose() {
        let input = "open ```sql\nSELECT 1\nlater ```text\nnote\n```\nend";
        assert_eq!(
            parse_message_segments(input),
            vec![
                text("open "),
                code("sql", "SELECT 1\nlater"),
                text("text\nnote\n```\nend"),
            ]
        );
        // The pure unclosed case: the whole input stays one text segment.
        assert_eq!(
            parse_message_segments("```sql\nSELECT 1"),
            vec![text("```sql\nSELECT 1")]
        );
    }

    #[test]
    fn detects_sql_language_tag() {
        assert!(is_sql_code_block("sql", "anything"));
        assert!(is_sql_code_block("postgresql", "x"));
    }

    #[test]
    fn detects_sql_by_content_when_no_language_tag() {
        assert!(is_sql_code_block("", "SELECT 1"));
        assert!(!is_sql_code_block("", "{\"a\":1}"));
    }

    #[test]
    fn rejects_non_sql_json_blocks() {
        assert!(!is_sql_code_block("json", "{\"a\":1}"));
    }

    /// Beyond the React suite — the word-boundary leg: `SELECT1` is not the
    /// SELECT keyword, and the case-insensitivity cuts both ways.
    #[test]
    fn the_leading_keyword_needs_a_word_boundary_and_ignores_case() {
        assert!(is_sql_code_block("", "  select * from t"));
        assert!(!is_sql_code_block("", "SELECT1 FROM t"));
        assert!(is_sql_code_block("", "with cte as (select 1) select * from cte"));
    }
}
