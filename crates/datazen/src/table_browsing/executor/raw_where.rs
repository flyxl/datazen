//! The raw `WHERE` predicate of a table read (DBR-04).
//!
//! The raw filter lets a user who knows SQL write one boolean predicate —
//! `total > 100 AND status = 'paid'` — instead of assembling it from the
//! structured conditions. The fragment is **user text spliced into a
//! statement**, so this module is the gate that decides whether it may run.
//!
//! The rules, all of them enforced by one scanner:
//!
//! * **One boolean expression, nothing else.** Statement verbs (`INSERT`,
//!   `UPDATE`, `DROP`, …) and statement-shaping clauses (`ORDER`, `GROUP`,
//!   `LIMIT`, `UNION`, `WITH`, …) are refused wherever they appear outside a
//!   quoted literal or a quoted identifier. A set-returning `WITH` is the
//!   subtle case that makes the list long: PostgreSQL allows a data-modifying
//!   CTE inside a subquery, so `WITH` may not pass even though a read-only
//!   CTE would be harmless.
//! * **One statement.** A `;` outside quotes is refused — including a
//!   trailing one, because the fragment is spliced into the middle of a
//!   statement the host owns.
//! * **No comments.** `--` and `/*` are refused outright. A comment cannot
//!   contribute anything to a predicate, but it can hide the rest of the
//!   line from a human reviewer.
//! * **Balanced parentheses and terminated literals.** An unbalanced `)`
//!   would close the host's own grouping early, and an unterminated quote
//!   would swallow the rest of the statement.
//! * **No dollar signs.** PostgreSQL dollar-quoting (`$$…$$`) is a second
//!   string syntax this scanner does not model, and `$1` placeholders bind
//!   to nothing here; both are refused rather than guessed at.
//!
//! What the scanner deliberately **allows**: `SELECT` (a scalar subquery is a
//! legitimate predicate), every boolean and function grammar, quoted strings
//! and quoted identifiers with their SQL escaping, and a `;` **inside** a
//! string literal (`note = 'a;b'` is a value, not a second statement).
//!
//! The validation is host-level and dialect-agnostic on purpose: the fragment
//! is spliced into a `SELECT` the host builds, so the host is the authority
//! for what may enter it. A driver that cannot trust this layer for its
//! grammar simply does not declare
//! [`supports_raw_where_filter`](datazen_driver_api::DatabaseDriver::supports_raw_where_filter)
//! and the mode is never offered for that database.

/// The longest raw predicate a request may carry.
///
/// The fragment is user text that ends up in a statement the host assembles,
/// so a bound belongs here in the shared layer: both the read and the preview
/// refuse the same number, and a preview can never show a statement the read
/// would then refuse for length.
pub const MAX_RAW_WHERE_LENGTH: usize = 2000;

/// Why a raw `WHERE` predicate may not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawWhereRejection {
    /// The fragment is empty (or only whitespace).
    Empty,
    /// The fragment exceeds [`MAX_RAW_WHERE_LENGTH`] characters.
    TooLong { len: usize },
    /// A `'…'` literal was still open at the end of the fragment.
    UnterminatedString { position: usize },
    /// A `"…"` quoted identifier was still open at the end of the fragment.
    UnterminatedQuotedIdentifier { position: usize },
    /// A `--` or `/*` comment; comments never contribute to a predicate.
    Comment { position: usize },
    /// A `;` outside quotes: the host owns the statement, the fragment may
    /// not start a second one.
    MultiStatement { position: usize },
    /// The parentheses do not balance. An unbalanced `)` would close the
    /// host's own grouping early; an unclosed `(` is the position reported.
    UnbalancedParenthesis { position: usize },
    /// A statement verb or statement-shaping clause outside quotes.
    ForbiddenKeyword { keyword: &'static str, position: usize },
    /// A `$` outside quotes: dollar-quoting is a string syntax this scanner
    /// does not model, and a `$1` placeholder binds to nothing here.
    DollarSign { position: usize },
}

impl RawWhereRejection {
    /// The message the host hands to the user.
    ///
    /// The contract prefix is `grid.filter.rawWhere*`, so the frontend can
    /// recognise the family and localise around it. The message names the
    /// rule and the character position; it never echoes the fragment, because
    /// a host message reaches the IPC payload and the log lines, and the
    /// redactor does not cover free text.
    pub fn contract_message(&self) -> String {
        match self {
            Self::Empty => "grid.filter.rawWhereEmpty: the raw filter is empty".into(),
            Self::TooLong { len } => format!(
                "grid.filter.rawWhereTooLong: the raw filter is {len} characters; the limit is {MAX_RAW_WHERE_LENGTH}"
            ),
            Self::UnterminatedString { position } => format!(
                "grid.filter.rawWhereUnterminatedString: the raw filter has a string literal that is not closed (character {position})"
            ),
            Self::UnterminatedQuotedIdentifier { position } => format!(
                "grid.filter.rawWhereUnterminatedIdentifier: the raw filter has a quoted column name that is not closed (character {position})"
            ),
            Self::Comment { position } => format!(
                "grid.filter.rawWhereComment: comments are not allowed in the raw filter (character {position})"
            ),
            Self::MultiStatement { position } => format!(
                "grid.filter.rawWhereMultiStatement: only one expression is allowed; the ';' at character {position} starts a second statement"
            ),
            Self::UnbalancedParenthesis { position } => format!(
                "grid.filter.rawWhereUnbalancedParenthesis: the parentheses of the raw filter do not balance (character {position})"
            ),
            Self::ForbiddenKeyword { keyword, position } => format!(
                "grid.filter.rawWhereForbiddenKeyword: '{keyword}' may not appear in the raw filter (character {position}); the fragment must be a single boolean expression"
            ),
            Self::DollarSign { position } => format!(
                "grid.filter.rawWhereDollarSign: '$' is not allowed in the raw filter (character {position})"
            ),
        }
    }
}

/// The statement verbs and statement-shaping clauses a predicate may not
/// carry, wherever they appear outside quotes.
///
/// Kept as a sorted slice so the membership check can binary-search, and so
/// a test can pin the ordering.
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "ALTER",
    "ANALYSE",
    "ANALYZE",
    "ATTACH",
    "CALL",
    "COMMENT",
    "COPY",
    "CREATE",
    "DELETE",
    "DETACH",
    "DROP",
    "EXCEPT",
    "EXEC",
    "EXECUTE",
    "EXPLAIN",
    "FETCH",
    "FOR",
    "GRANT",
    "GROUP",
    "INSERT",
    "INTERSECT",
    "INTO",
    "LIMIT",
    "LOCK",
    "MERGE",
    "OFFSET",
    "ORDER",
    "PRAGMA",
    "REINDEX",
    "RENAME",
    "REPLACE",
    "RESET",
    "REVOKE",
    "SET",
    "TRUNCATE",
    "UNION",
    "UNLOCK",
    "UPDATE",
    "USE",
    "VACUUM",
    "WITH",
];

/// Whether the fragment is a predicate the host may splice into a `WHERE`.
///
/// Pure text work: no dialect, no driver, no I/O. The caller is expected to
/// have trimmed the input; an empty fragment is [`RawWhereRejection::Empty`]
/// so the standalone contract is total (the read path maps an empty fragment
/// to "no raw filter" before it gets here).
pub fn validate_raw_where(input: &str) -> Result<(), RawWhereRejection> {
    let chars: Vec<char> = input.chars().collect();
    if chars.iter().all(|character| character.is_whitespace()) {
        return Err(RawWhereRejection::Empty);
    }
    if input.chars().count() > MAX_RAW_WHERE_LENGTH {
        return Err(RawWhereRejection::TooLong {
            len: input.chars().count(),
        });
    }

    // The open parentheses, as character positions, so an unbalanced fragment
    // can point at the `(` that was never closed.
    let mut open_parens: Vec<usize> = Vec::new();
    let mut word = String::new();
    let mut word_start = 0usize;

    let mut flush_word = |word: &mut String, word_start: usize| -> Result<(), RawWhereRejection> {
        if word.is_empty() {
            return Ok(());
        }
        let upper = word.to_ascii_uppercase();
        if FORBIDDEN_KEYWORDS.binary_search(&upper.as_str()).is_ok() {
            return Err(RawWhereRejection::ForbiddenKeyword {
                keyword: FORBIDDEN_KEYWORDS
                    .iter()
                    .find(|keyword| **keyword == upper)
                    .copied()
                    .unwrap_or(""),
                position: word_start,
            });
        }
        word.clear();
        Ok(())
    };

    let mut index = 0usize;
    while index < chars.len() {
        let character = chars[index];
        match character {
            '\'' => {
                flush_word(&mut word, word_start)?;
                // A string literal: `''` is an escaped quote, anything else
                // up to the next `'` is value text — including `;`, `--` and
                // newlines, none of which the scanner judges inside a value.
                let start = index;
                index += 1;
                loop {
                    match chars.get(index) {
                        None => {
                            return Err(RawWhereRejection::UnterminatedString { position: start });
                        }
                        Some('\'') if chars.get(index + 1) == Some(&'\'') => {
                            index += 2;
                        }
                        Some('\'') => {
                            index += 1;
                            break;
                        }
                        Some(_) => {
                            index += 1;
                        }
                    }
                }
            }
            '"' => {
                flush_word(&mut word, word_start)?;
                // A quoted identifier: same escaping rule with `""`.
                let start = index;
                index += 1;
                loop {
                    match chars.get(index) {
                        None => {
                            return Err(RawWhereRejection::UnterminatedQuotedIdentifier {
                                position: start,
                            });
                        }
                        Some('"') if chars.get(index + 1) == Some(&'"') => {
                            index += 2;
                        }
                        Some('"') => {
                            index += 1;
                            break;
                        }
                        Some(_) => {
                            index += 1;
                        }
                    }
                }
            }
            '-' if chars.get(index + 1) == Some(&'-') => {
                return Err(RawWhereRejection::Comment { position: index });
            }
            '/' if chars.get(index + 1) == Some(&'*') => {
                return Err(RawWhereRejection::Comment { position: index });
            }
            ';' => {
                flush_word(&mut word, word_start)?;
                return Err(RawWhereRejection::MultiStatement { position: index });
            }
            '$' => {
                flush_word(&mut word, word_start)?;
                return Err(RawWhereRejection::DollarSign { position: index });
            }
            '(' => {
                flush_word(&mut word, word_start)?;
                open_parens.push(index);
                index += 1;
            }
            ')' => {
                flush_word(&mut word, word_start)?;
                if open_parens.pop().is_none() {
                    return Err(RawWhereRejection::UnbalancedParenthesis { position: index });
                }
                index += 1;
            }
            _ if character.is_ascii_alphabetic() || character == '_' => {
                if word.is_empty() {
                    word_start = index;
                }
                word.push(character);
                index += 1;
            }
            _ => {
                flush_word(&mut word, word_start)?;
                // Operators, digits, dots, commas, whitespace: the boolean
                // expression grammar the fragment is allowed to use.
                index += 1;
            }
        }
    }
    flush_word(&mut word, word_start)?;

    if let Some(position) = open_parens.first() {
        return Err(RawWhereRejection::UnbalancedParenthesis { position: *position });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejects(input: &str) -> RawWhereRejection {
        validate_raw_where(input).expect_err("the fragment must be refused")
    }

    fn accepts(input: &str) {
        validate_raw_where(input).unwrap_or_else(|rejection| {
            panic!("the fragment `{input}` must be accepted, got {rejection:?}")
        });
    }

    #[test]
    fn empty_and_whitespace_only_fragments_are_refused() {
        assert_eq!(rejects(""), RawWhereRejection::Empty);
        assert_eq!(rejects("   \n\t "), RawWhereRejection::Empty);
    }

    #[test]
    fn a_plain_predicate_is_accepted() {
        accepts("total > 100 AND status = 'paid'");
        accepts("\"order\".total >= 10");
        accepts("name LIKE 'a%'");
        accepts("created_at BETWEEN '2026-01-01' AND '2026-12-31'");
        accepts("notes IS NOT NULL AND notes != ''");
    }

    #[test]
    fn a_subquery_is_accepted() {
        accepts("total > (SELECT AVG(total) FROM orders)");
        accepts(
            "customer_id IN (SELECT id FROM customers WHERE tier = 'gold' AND (region = 'eu' OR region = 'us'))",
        );
    }

    #[test]
    fn injection_attempts_are_refused() {
        for fragment in [
            "1 = 1; DELETE FROM users",
            "1 = 1; DROP TABLE users",
            "1 = 1;--",
            "1 = 1 /* comment */ AND 2 = 2",
            "1 = 1) OR (2 = 2",
            "true AND 1 = 1 UNION SELECT password FROM users",
            "x IN (WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d)",
            "x IN (SELECT 1) ORDER BY 1",
            "x IN (SELECT 1) LIMIT 1",
            "x = 1 OR total > 1 GROUP BY x",
            "name = 'a' || $quote$; DROP TABLE x$quote$",
        ] {
            assert!(
                validate_raw_where(fragment).is_err(),
                "the injection `{fragment}` must be refused"
            );
        }
    }

    #[test]
    fn a_semicolon_inside_a_literal_is_a_value() {
        accepts("note = 'a;b'");
        accepts("note = 'it''s; fine'");
    }

    #[test]
    fn keywords_inside_literals_are_values() {
        accepts("note = 'DELETE FROM users'");
        accepts("\"delete\" = 'x'");
    }

    #[test]
    fn every_write_and_shaping_keyword_is_refused_with_its_position() {
        for (fragment, keyword, position) in [
            ("x = 1 AND DELETE", "DELETE", 10),
            ("UPDATE t SET x = 1", "UPDATE", 0),
            ("x = 1 ORDER BY y", "ORDER", 6),
            ("x = 1 LIMIT 1", "LIMIT", 6),
            ("x = 1 WITH c AS (SELECT 1) SELECT 2", "WITH", 6),
            ("x = 1 FOR UPDATE", "FOR", 6),
        ] {
            assert_eq!(
                rejects(fragment),
                RawWhereRejection::ForbiddenKeyword { keyword, position },
                "the fragment `{fragment}` must name {keyword}"
            );
        }
    }

    #[test]
    fn the_forbidden_list_is_sorted_and_has_no_duplicates() {
        // The membership check binary-searches; an unsorted list would silently
        // stop finding half the keywords.
        let mut sorted = FORBIDDEN_KEYWORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, FORBIDDEN_KEYWORDS);
        assert!(FORBIDDEN_KEYWORDS.len() > 30);
    }

    #[test]
    fn unbalanced_parentheses_point_at_the_culprit() {
        assert_eq!(
            rejects("x = (SELECT 1"),
            RawWhereRejection::UnbalancedParenthesis { position: 4 },
        );
        assert_eq!(
            rejects("x = 1) AND (y = 2"),
            RawWhereRejection::UnbalancedParenthesis { position: 5 },
        );
    }

    #[test]
    fn unterminated_literals_are_refused() {
        assert_eq!(
            rejects("note = 'open"),
            RawWhereRejection::UnterminatedString { position: 7 },
        );
        assert_eq!(
            rejects("\"col = 1"),
            RawWhereRejection::UnterminatedQuotedIdentifier { position: 0 },
        );
    }

    #[test]
    fn dollar_signs_are_refused() {
        assert_eq!(
            rejects("x = $$abc$$"),
            RawWhereRejection::DollarSign { position: 4 },
        );
    }

    #[test]
    fn overlong_fragments_are_refused() {
        let long = "a".repeat(MAX_RAW_WHERE_LENGTH + 1);
        assert_eq!(
            validate_raw_where(&long),
            Err(RawWhereRejection::TooLong {
                len: MAX_RAW_WHERE_LENGTH + 1
            }),
        );
        let boundary = "a".repeat(MAX_RAW_WHERE_LENGTH);
        accepts(&boundary);
    }

    #[test]
    fn contract_messages_carry_the_family_prefix() {
        for rejection in [
            RawWhereRejection::Empty,
            RawWhereRejection::TooLong { len: 42 },
            RawWhereRejection::UnterminatedString { position: 3 },
            RawWhereRejection::UnterminatedQuotedIdentifier { position: 3 },
            RawWhereRejection::Comment { position: 3 },
            RawWhereRejection::MultiStatement { position: 3 },
            RawWhereRejection::UnbalancedParenthesis { position: 3 },
            RawWhereRejection::ForbiddenKeyword {
                keyword: "DELETE",
                position: 3,
            },
            RawWhereRejection::DollarSign { position: 3 },
        ] {
            let message = rejection.contract_message();
            // The literals are concatenated so this test's own source does
            // not read as a code the registration scan would have to
            // register (same discipline as the api.rs signature guard).
            let prefix = ["grid.filter.", "rawWhere"].concat();
            assert!(
                message.starts_with(&prefix),
                "the message `{message}` must carry the contract prefix"
            );
        }
    }
}
